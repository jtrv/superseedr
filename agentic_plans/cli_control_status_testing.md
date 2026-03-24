# Shared-Config CLI Feature Validation Matrix: codex/unified-config

## Purpose

This is a focused feature-validation plan for the shared-config and unified-config behavior in this branch.

It is not a full branch-wide regression plan.

This plan assumes the following already exist elsewhere:
- unit tests for internal logic
- interop or integration testing for broader runtime behavior
- normal-mode coverage outside this document

This plan validates:
- shared-config activation and routing
- all CLI commands in shared mode
- both single-machine shared mode and optional concurrent shared-cluster mode

This plan does not require a full end-to-end torrent lifecycle such as real download or seeding.

## Scope

Validate these shared-config feature behaviors:

1. env-driven shared activation
2. launcher shared-config commands
3. shared-root normalization
4. add routing into the shared inbox
5. all CLI commands in shared mode
6. host-id separation on one machine
7. docs matching actual shared-config behavior
8. optional concurrent cluster proof for the shared CLI surface

## Out Of Scope

Do not spend time re-running broad coverage already handled by unit tests or interop testing.

This plan does not require:
- full normal-mode regression
- full end-to-end download and seeding validation
- tracker/network correctness
- deep TUI walkthroughs unrelated to shared-config
- full migration validation unless explicitly required
- broad resilience matrices unless explicitly required
- full multi-node interop scenarios beyond the shared CLI surface

## Core Execution Rule

- The agent must test the current checked-out codebase by running cargo run, not an installed global binary.
- Prefer cargo run -- <args> for CLI validation.
- Prefer env-prefixed cargo run -- <args> for shared-mode validation.
- Prefer env-prefixed cargo run for launching the current TUI in shared mode.
- Do not assume a previously installed binary matches the current checkout.

## Workspace And Shared Root Rules

- Use the current working directory's ./tmp/ as the default shared mount root for this plan.
- Treat ./tmp/ as both:
  - the scratch workspace for generated validation artifacts
  - the default local shared-config mount root for shared-mode tests
- Do not scatter scratch files elsewhere in the repository.
- Do not commit ./tmp/ contents.

Recommended layout:
- ./tmp/superseedr-config/hosts/
- ./tmp/superseedr-config/inbox/
- ./tmp/superseedr-config/processed/
- ./tmp/superseedr-config/status/
- ./tmp/superseedr-config/torrents/
- ./tmp/evidence/
- ./tmp/reports/

## Asset Reuse Rules

- Prefer integration_tests/ over any generated assets when suitable torrent or payload fixtures exist there.
- Only fall back to other fixture directories if integration_tests/ is absent or insufficient.
- Only generate temporary assets under ./tmp/ if the repo does not already contain suitable reusable fixtures.
- Record exactly which files were reused.

## How To Run Shared Mode With Env Vars

Use env-driven launches for the main flow. Do not use set-shared-config as the default activation path.

Unix-like examples:
- SUPERSEEDR_SHARED_CONFIG_DIR="$(pwd)/tmp" cargo run
- SUPERSEEDR_SHARED_CONFIG_DIR="$(pwd)/tmp" cargo run -- show-shared-config
- SUPERSEEDR_SHARED_CONFIG_DIR="$(pwd)/tmp" SUPERSEEDR_SHARED_HOST_ID="host-a" cargo run -- show-shared-config
- SUPERSEEDR_SHARED_CONFIG_DIR="$(pwd)/tmp" SUPERSEEDR_SHARED_HOST_ID="host-a" cargo run -- add "magnet:?xt=..."
- SUPERSEEDR_SHARED_CONFIG_DIR="$(pwd)/tmp" SUPERSEEDR_SHARED_HOST_ID="host-a" cargo run -- status

PowerShell:
- $env:SUPERSEEDR_SHARED_CONFIG_DIR = "$PWD\tmp"
- $env:SUPERSEEDR_SHARED_HOST_ID = "host-a"
- cargo run
- cargo run -- show-shared-config

cmd.exe:
- set SUPERSEEDR_SHARED_CONFIG_DIR=%cd%\tmp
- set SUPERSEEDR_SHARED_HOST_ID=host-a
- cargo run
- cargo run -- show-shared-config

Expected shared-mode result:
- show-shared-config reports source env
- mount root resolves to ./tmp
- config root resolves to ./tmp/superseedr-config
- command routing uses ./tmp/superseedr-config/inbox/

## Required Test Data

Prepare only what is needed for this feature validation:
- at least one reusable .torrent fixture from integration_tests/ if present
- at least one fabricated magnet string for routing validation if no real repo magnet exists
- one shared root at ./tmp

If only a fabricated magnet is used, record clearly that this validates queueing and routing only, not a full real magnet ingest lifecycle.

## Command Matrix

Use this matrix to drive execution and reporting.

Columns:
- Single Shared Offline: command run with shared env vars, no running instance
- Single Shared Online: command run with shared env vars, one running shared instance
- Cluster Shared Online: command run with two runtimes on same shared root
- Required: Yes means required for this plan; Optional means only if environment supports it
- Validation Goal: what is being proven

| Command | Single Shared Offline | Single Shared Online | Cluster Shared Online | Required | Validation Goal |
|---|---:|---:|---:|---|---|
| show-shared-config | Yes | Yes | Yes | Yes | Env/launcher selection reports correct shared mode |
| set-shared-config | N/A | N/A | N/A | Yes | Launcher persistence command works |
| clear-shared-config | N/A | N/A | N/A | Yes | Launcher clear command works |
| add | Yes | Yes | Yes | Yes | Routes into shared inbox / shared command path |
| status | Yes | Yes | Yes | Yes | Shared-mode status output works |
| journal | Yes | Yes | Yes | Yes | Shared-mode journal output works |
| torrents | Yes | Yes | Yes | Yes | Read-only shared-mode query works |
| info | Yes | Yes | Yes | Yes | Read-only shared-mode query works |
| files | Yes | Yes | Yes | Yes | Read-only shared-mode query works |
| pause | Yes | Yes | Yes | Yes | Shared-mode control path works |
| resume | Yes | Yes | Yes | Yes | Shared-mode control path works |
| remove | Yes | Yes | Yes | Yes | Shared-mode control path works |
| purge | Yes | Yes | Yes | Yes | Shared-mode control path works without full download cycle |
| priority | Yes | Yes | Yes | Yes | Shared-mode control path works |
| stop-client | No | Yes | Yes | Yes | Runtime stop path works against live shared instance |

Notes:
- N/A means the command is not meaningfully “offline vs online shared runtime”; test it in its own launcher-command section.
- Cluster Shared Online is required only when the environment supports it.
- No command in this plan requires a successful download, metadata fetch, or seeding cycle.

## Validation Levels

For each command, record one or more of these validation levels:

- accepted: command parses and runs
- routed: command reaches the correct shared-mode path
- queued: command writes to shared inbox or command sink correctly
- applied: command changes shared or host-local state as intended
- observed: result is visible in status, files, journal, or filesystem
- cluster-observed: result is visible from a second runtime on the same shared root

A command should not be marked “fully validated” unless the report states which of the above levels were observed.

## Execution Phases

## Phase 1: Shared-Mode Environment And Layout

## 1. Env-Driven Shared Activation

### Goal
Prove that the branch enters shared mode from env vars without relying on persisted launcher config.

### Steps
1. Ensure SUPERSEEDR_SHARED_CONFIG_DIR is unset.
2. Run cargo run -- show-shared-config and record the baseline.
3. Run SUPERSEEDR_SHARED_CONFIG_DIR="$(pwd)/tmp" cargo run -- show-shared-config.
4. Repeat with SUPERSEEDR_SHARED_HOST_ID=host-a.

### Expected
- env-driven show-shared-config reports enabled
- source is env
- mount root is ./tmp
- config root is ./tmp/superseedr-config

## 2. Shared Root Normalization

### Goal
Prove that both mount-root and explicit superseedr-config forms resolve correctly.

### Steps
1. Run with SUPERSEEDR_SHARED_CONFIG_DIR pointing at the absolute path of ./tmp.
2. Run again with SUPERSEEDR_SHARED_CONFIG_DIR pointing at the absolute path of ./tmp/superseedr-config.
3. Compare show-shared-config output.

### Expected
- both forms resolve correctly
- no duplicated nested config root

## 3. Shared File Layout Smoke

### Goal
Prove that the branch creates and uses the expected shared layout.

### Steps
1. Launch the client once in env-driven shared mode with cargo run.
2. Inspect ./tmp/superseedr-config/.

### Expected
Relevant layout exists as needed:
- hosts/
- inbox/
- processed/
- status/
- torrents/
- settings.toml
- torrent_metadata.toml
- catalog.toml if created by flow
- lock file if applicable

## Phase 2: Single-Machine Shared CLI Matrix

Run these tests on one machine against ./tmp as the shared root.

## 4. Shared Read-Only Command Matrix

### Commands
- show-shared-config
- status
- journal
- torrents
- info
- files

### Required contexts
- offline shared CLI: required
- online shared runtime: required

### Expected
- each command runs successfully or fails with a correct and understandable reason
- output shape is correct in both text and JSON where supported
- read commands do not mutate unrelated shared state

## 5. Shared Mutating Command Matrix

### Commands
- add
- pause
- resume
- remove
- purge
- priority
- stop-client

### Required contexts
- offline shared CLI: required for all except stop-client
- online shared runtime: required for all
- cluster shared online: optional unless environment supports it

### Expected
- each command reaches the correct shared-mode path
- commands that should queue do queue to shared infrastructure
- commands that should mutate shared or host-local state do so in the correct scope
- no command accidentally falls back to normal local routing

### Important note
This section does not require:
- tracker success
- metadata fetch success
- download completion
- seeding validation

It does require:
- correct command acceptance
- correct routing
- correct shared-mode storage or control behavior

## 6. Add Routing Details

### Goal
Prove that add requests route into the shared inbox.

### Steps
1. In env-driven shared mode, run cargo run -- add "<magnet>".
2. In env-driven shared mode, run cargo run -- add "<torrent-path>" using a reused fixture from integration_tests/ if present.
3. Inspect ./tmp/superseedr-config/inbox/.

### Expected
- magnet add lands in the shared inbox
- torrent add lands in the shared inbox, typically as a .path file
- add does not use the normal local watch sink

### Required note
- If cargo run -- add was tested instead of positional direct input, record that clearly.
- If positional direct input was not tested, record that gap rather than implying it was covered.

## 7. Host-ID Separation On One Machine

### Goal
Prove that host-scoped files separate correctly without requiring two concurrent machines.

### Steps
1. Run the client against ./tmp with SUPERSEEDR_SHARED_HOST_ID=host-a.
2. Quit cleanly.
3. Run the client again against the same shared root with SUPERSEEDR_SHARED_HOST_ID=host-b.
4. Inspect:
   - ./tmp/superseedr-config/hosts/
   - ./tmp/superseedr-config/status/

### Expected
- host-a.toml and host-b.toml can coexist
- status files are host-separated when produced
- shared global files remain shared

### Required explicit check
- List the hosts directory and record that both host-a.toml and host-b.toml exist.

## 8. Launcher Command Matrix

### Commands
- set-shared-config
- clear-shared-config
- show-shared-config

### Goal
Prove that launcher shared-config commands themselves work, without using them as the default test path.

### Steps
1. Record cargo run -- show-shared-config.
2. Run cargo run -- set-shared-config <absolute-path-to-tmp>.
3. Run cargo run -- show-shared-config and verify launcher source.
4. Run cargo run -- clear-shared-config.
5. Run cargo run -- show-shared-config again.

### Expected
- set-shared-config works
- show-shared-config shows launcher after set
- clear-shared-config works
- show-shared-config returns to disabled or baseline state after clear

### Cleanup requirement
- Always clear after this section unless the environment explicitly requires persistence to remain.

## Phase 3: Optional Concurrent Shared-Cluster Matrix

Only run if the environment supports two active runtimes.

## 9. Minimal Concurrent Shared-Cluster Setup

### Goal
Create a real concurrent shared-mode environment sufficient to validate the shared CLI surface.

### Acceptable environments
- two machines with a mounted shared directory
- one native cargo run instance plus one Docker or container instance sharing the same mounted host directory
- two containers sharing the same mounted host directory

### Shared directory
Use a dedicated concurrent shared root such as:
- ./tmp-cluster-share/

Runtime A:
- SUPERSEEDR_SHARED_CONFIG_DIR points at the shared mount path seen by runtime A
- SUPERSEEDR_SHARED_HOST_ID=host-a

Runtime B:
- SUPERSEEDR_SHARED_CONFIG_DIR points at the same shared contents as seen by runtime B
- SUPERSEEDR_SHARED_HOST_ID=host-b

If using Docker:
- mount the same host directory into the container
- set SUPERSEEDR_SHARED_CONFIG_DIR inside the container to the mounted path
- preserve distinct host IDs

### Pre-flight checks
- both runtimes can create files in the shared root
- files written by one runtime are visible to the other
- both runtimes resolve the same shared config layout
- host IDs differ

## 10. Concurrent Shared Read-Only Command Matrix

### Commands
- status
- journal
- torrents
- info
- files
- show-shared-config

### Goal
Prove that shared-mode read commands work when leader and follower are both active.

### Expected
- commands run from cluster mode
- output is sensible from both runtimes when applicable
- results reflect shared cluster state

## 11. Concurrent Shared Mutating Command Matrix

### Commands
- add
- pause
- resume
- remove
- purge
- priority
- stop-client

### Goal
Prove that the shared CLI control surface works when two runtimes are active.

### Steps
1. Start runtime A.
2. Start runtime B.
3. Confirm one leader and one follower.
4. Run each in-scope mutating CLI command from at least one side.
5. For a subset, run them from both sides.
6. Observe shared files and command effects.

### Expected
- both runtimes see the same shared files
- CLI commands operate through the cluster shared-config path
- follower-issued commands do not accidentally use local normal-mode routing
- no command requires a full download/seeding cycle to be considered validated here

## 12. Minimum Concurrent Proof Set

If time is limited, at minimum validate these cluster commands:
- add
- status
- pause
- resume
- remove or purge
- stop-client

## 13. Docs Match Actual Behavior

### Goal
Verify that the docs for the feature are accurate.

### Review
- README.md
- docs/shared-config.md

### Confirm
- env-driven activation is documented correctly
- launcher commands match actual behavior
- env precedence is described correctly
- shared root layout matches observed behavior
- host vs shared settings scope matches observed behavior
- CLI surface described for shared mode is accurate

## Good Additional Behaviors To Preserve

These are good practice and should remain in agent runs:

1. Cleanup after launcher-config testing
- after set-shared-config, run clear-shared-config unless the environment explicitly requires persistence to remain

2. Verify clear actually worked
- after clear-shared-config, run show-shared-config again and confirm the expected cleared state

3. Test both text and JSON status output
- shared-mode status should be checked in both text and JSON-wrapper forms when possible

4. Explicit filesystem verification
- when testing host-id separation, explicitly inspect the hosts/ directory and confirm both host files exist

5. Write the report to disk
- create a report path under ./tmp/reports/ and write the final validation report there, not just stdout

6. Record when add was tested through explicit add syntax
- if cargo run -- add "magnet:..." is used instead of positional input, note that clearly
- if positional direct input is not tested, record that gap

7. Record magnet quality honestly
- if only a fabricated magnet string was used, state that it validates queueing and routing only, not a full real magnet ingest lifecycle

## Out Of Scope By Default

These are out of scope for this doc unless explicitly requested:
- full normal-mode validation
- full end-to-end download and seeding validation
- tracker/network correctness
- full TUI validation
- full migration validation
- resilience matrix
- corruption matrix
- broad Docker validation
- full multi-node interop validation

## Evidence To Record

Store under ./tmp/reports/ and ./tmp/evidence/:
- exact commands run through cargo run
- exact fixture paths reused from integration_tests/ if any
- inbox file paths created by add routing
- host file paths created for host-a and host-b
- show-shared-config outputs
- concise notes on what was proven versus only partially validated
- which CLI commands were validated in:
  - single-machine shared offline
  - single-machine shared online
  - concurrent cluster shared online
- which commands were only validated as routing or queueing checks

## Report Matrix

Use this table shape in the final report.

| Command | Single Shared Offline | Single Shared Online | Cluster Shared Online | Validation Level | Notes |
|---|---|---|---|---|---|
| show-shared-config |  |  |  |  |  |
| set-shared-config |  |  | N/A |  |  |
| clear-shared-config |  |  | N/A |  |  |
| add |  |  |  |  |  |
| status |  |  |  |  |  |
| journal |  |  |  |  |  |
| torrents |  |  |  |  |  |
| info |  |  |  |  |  |
| files |  |  |  |  |  |
| pause |  |  |  |  |  |
| resume |  |  |  |  |  |
| remove |  |  |  |  |  |
| purge |  |  |  |  |  |
| priority |  |  |  |  |  |
| stop-client | N/A |  |  |  |  |

Suggested values:
- Pass
- Fail
- Skipped
- N/A

Validation Level examples:
- accepted
- routed
- queued
- applied
- observed
- cluster-observed

## Output Format For Agent Report

## Summary
- overall result: pass, pass with issues, or fail
- highest-severity finding
- confidence level
- which shared-mode CLI sections were completed

## Command Matrix
- include the completed report matrix

## Passed
- shared-mode CLI checks that passed

## Failed
For each failure:
- title
- severity
- exact reproduction
- expected vs actual
- likely affected files
- evidence paths

## Skipped
- skipped because environment did not support concurrency
- skipped because out of scope for this feature-validation plan
- skipped because already covered elsewhere and not needed for branch-specific proof

## Important Caveats
- whether magnet validation used a real or fabricated magnet
- whether positional direct input was tested separately from add
- whether concurrent leader/follower proof was run
- whether a command was validated only as queueing/routing versus a live shared runtime action

## Release Recommendation
Choose one:
- ready for merge from shared-cli feature-validation perspective
- ready after small shared-cli feature fixes
- needs another focused shared-cli pass
- not ready
