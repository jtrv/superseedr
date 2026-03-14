# Shared Config Mode

## Overview
Superseedr supports an opt-in shared config mode for operators who want multiple machines to point at the same mounted config directory while keeping per-machine runtime state local.

Shared config mode is enabled only when `SUPERSEEDR_SHARED_CONFIG_DIR` is set.
If the env var is not set, Superseedr uses the normal single-file `settings.toml` flow in the platform config directory.

## Environment Variables

### `SUPERSEEDR_SHARED_CONFIG_DIR`
Absolute path to the shared config root.

When set, Superseedr loads configuration from:
- `settings.toml`
- `catalog.toml`
- `hosts/<host-id>.toml`

Example:

```bash
SUPERSEEDR_SHARED_CONFIG_DIR=/mnt/superseedr-config
```

### `SUPERSEEDR_HOST_ID`
Optional explicit host id for picking the host override file.

When set, Superseedr loads:

```text
hosts/<SUPERSEEDR_HOST_ID>.toml
```

When not set, Superseedr falls back to a sanitized hostname.

Example:

```bash
SUPERSEEDR_HOST_ID=seedbox-a
```

## Shared Mode Layout

```text
/mnt/superseedr-config/
  settings.toml
  catalog.toml
  hosts/
    seedbox-a.toml
    windows-node.toml
```

### `settings.toml`
Shared non-torrent settings live here:
- shared `client_id` default
- RSS settings
- shared UI and performance settings
- shared default download location

### `catalog.toml`
Shared torrent catalog lives here:
- torrent list
- torrent-level download targets and state that belongs to the shared catalog

### `torrents/`
Canonical shared `.torrent` files live here for file-based torrents so every host can load the same artifact from the mounted config root.

### `hosts/<host-id>.toml`
Machine-specific values live here:
- optional `client_id` override
- `client_port`
- `watch_folder`
- `path_roots`

## Path Handling
Shared mode supports two path forms for shared settings and catalog-owned paths.

### Absolute path
Useful when every machine sees the same absolute path.

```toml
default_download_folder = "/srv/downloads"
```

### Portable path
Useful when different machines mount the same storage at different locations.

```toml
default_download_folder = { root = "media", relative = "downloads" }
```

Then each host maps that root locally:

```toml
[path_roots]
media = "/mnt/nas"
```

On another machine:

```toml
[path_roots]
media = "Z:\\nas"
```

Portable path support currently applies to:
- `default_download_folder` in `settings.toml`
- per-torrent `download_path` in `catalog.toml`

If a portable root is missing from `path_roots`, Superseedr fails with a clear error instead of guessing.

## What Stays Local
Shared mode does not move runtime persistence into the mounted config directory.
These files remain in the normal local app data directory:
- logs
- lock file
- `persistence/rss.toml`
- `persistence/network_history.bin`
- activity history persistence
- local watch/processed command files created under the app data dir

This keeps shared config focused on desired state instead of mixing in per-instance cache, telemetry, and diagnostics.

## Write Behavior
In shared mode:
- shared non-torrent settings save to `settings.toml`
- torrents save to `catalog.toml`
- host-local settings save to `hosts/<host-id>.toml`
- shared path fields are manual-edit-only in the app for now

The config screen points users to `settings.toml` for shared default download path edits.

## Stale Write Protection
Shared mode protects against silent overwrite when multiple machines edit the same shared files.

Before saving, Superseedr checks whether `settings.toml`, `catalog.toml`, or the host file changed on disk since they were loaded.
If they changed, the save is rejected and the app reports that a reload is required.

## Example
Shared settings:

```toml
# settings.toml
client_id = "shared-node"
default_download_folder = { root = "media", relative = "downloads" }
global_upload_limit_bps = 8000000
```

Shared catalog:

```toml
# catalog.toml
[[torrents]]
name = "Shared Collection"
download_path = { root = "media", relative = "downloads/shared" }
```

Linux host:

```toml
# hosts/seedbox-a.toml
client_port = 6681
watch_folder = "/mnt/nas/watch"

[path_roots]
media = "/mnt/nas"
```

Windows host:

```toml
# hosts/windows-node.toml
client_port = 6681
watch_folder = "Z:\\watch"

[path_roots]
media = "Z:\\nas"
```

## Recommended Setups

### Single machine, no shared mode
Best for:
- one machine
- the simplest setup
- no shared catalog

Use the normal default mode and do not set `SUPERSEEDR_SHARED_CONFIG_DIR`.
Superseedr will keep using its standard OS config directory and single `settings.toml`.

Launch:

```powershell
cargo run
```

### Windows and macOS sharing one mounted seedbox folder
Best for:
- one shared data folder
- Windows and macOS using different absolute paths
- one shared torrent catalog

Recommended layout:

```text
seedbox/
  superseedr-config/
    settings.toml
    catalog.toml
    torrents/
    hosts/
      jagas-air.toml
      desktop-0mtgcbo.toml
```

Recommended shared root placement:
- put `superseedr-config/` inside the mounted data folder
- this lets first-run host bootstrap infer `media` as the parent folder

Shared settings:

```toml
# settings.toml
client_id = "shared-node"
default_download_folder = { root = "media", relative = "" }
```

Mac host:

```toml
# hosts/jagas-air.toml
client_port = 6681
watch_folder = "/Volumes/seedbox/watch"

[path_roots]
media = "/Volumes/seedbox"
```

Windows host:

```toml
# hosts/desktop-0mtgcbo.toml
client_port = 6681
watch_folder = "C:\\Users\\jagat\\Documents\\seedbox\\watch"

[path_roots]
media = "C:\\Users\\jagat\\Documents\\seedbox"
```

Launch on macOS:

```bash
SUPERSEEDR_SHARED_CONFIG_DIR="/Volumes/seedbox/superseedr-config" cargo run
```

Launch on Windows:

```powershell
$env:SUPERSEEDR_SHARED_CONFIG_DIR='C:\Users\jagat\Documents\seedbox\superseedr-config'; cargo run
```

Notes:
- If the host file does not exist yet, Superseedr bootstraps it on first load.
- If a host file already exists, Superseedr trusts it and does not overwrite existing `path_roots`.

### Docker using a shared mount
Best for:
- a containerized seedbox
- shared config under the same mounted data root

Recommended container mount:

```text
/seedbox/
  superseedr-config/
  watch/
  downloads/
```

Recommended run command:

```bash
docker run \
  -e SUPERSEEDR_SHARED_CONFIG_DIR=/seedbox/superseedr-config \
  -e SUPERSEEDR_HOST_ID=seedbox-docker \
  -v /real/seedbox:/seedbox \
  your-image
```

Why this layout is recommended:
- first-run bootstrap infers `media` as `/seedbox`
- shared `.torrent` artifacts stay under `/seedbox/superseedr-config/torrents/`
- all hosts can resolve the same shared torrent artifacts

If config and data are mounted separately, do this instead:
- create `hosts/<host-id>.toml` manually
- set `[path_roots]` explicitly
- do not rely on first-run bootstrap to infer the data root

### Shared catalog safety guidance
Recommended operational model:
- share config and catalog across hosts
- keep runtime persistence local
- prefer one active owner per torrent unless stronger ownership coordination is added later

Why:
- shared config sync keeps hosts converged
- runtime persistence is intentionally local
- tracker behavior is safer when one host is actively responsible for a torrent at a time
## Migration Script
A one-time migration helper is available at `local_scripts/migrate_legacy_settings_to_layered.py`.

It reads a legacy flat `settings.toml` and writes:
- shared `settings.toml`
- shared `catalog.toml`
- `hosts/<host-id>.toml`
- canonical shared `.torrent` copies under `torrents/` when possible

Example:

```bash
python3 local_scripts/migrate_legacy_settings_to_layered.py \
  --input "/path/to/old/settings.toml" \
  --shared-root "/Volumes/seedbox/superseedr-config" \
  --host-id "seedbox" \
  --path-root media=/Volumes/seedbox \
  --force
```

Notes:
- The script converts `default_download_folder` and per-torrent `download_path` through the `--path-root` mappings you provide.
- Magnet entries are preserved as-is.
- File-based torrent entries are copied into `torrents/` only when the source filename already uses the expected 40-character hex info-hash stem.
- If a file-based torrent source is missing or its filename stem is not the expected info hash, the script warns and keeps the original source path instead of inventing a broken shared artifact reference.

## Notes
- Shared config mode is opt-in.
- No automatic migration is performed from the normal `settings.toml` layout.
- Shared config mode is only about config sharing. It does not add multi-instance torrent ownership or execution coordination.



