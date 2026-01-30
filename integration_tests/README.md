# Superseedr Integration Testing Guide

<!-- SPDX-FileCopyrightText: 2025 The superseedr Contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

This guide explains how to run and extend the integration test suite for Superseedr.

## Table of Contents

- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Running Tests Locally](#running-tests-locally)
- [Test Scenarios](#test-scenarios)
- [Architecture](#architecture)
- [Troubleshooting](#troubleshooting)
- [Extending Tests](#extending-tests)

## Overview

The integration test suite verifies interoperability between Superseedr and reference BitTorrent clients (qBittorrent) across v1, v2, and hybrid torrent protocols. It uses Docker Compose to orchestrate a complete testing environment including:

- **Tracker**: A private BitTorrent tracker (opentracker)
- **Superseedr**: The application under test
- **qBittorrent**: Reference client for compatibility testing

## Prerequisites

### Required Software

1. **Docker & Docker Compose**
   ```bash
   # macOS
   brew install docker docker-compose

   # Ubuntu/Debian
   sudo apt-get install docker.io docker-compose

   # Or use Docker Desktop for Mac/Windows
   ```

2. **Python 3.11+**
   ```bash
   # macOS
   brew install python@3.11

   # Ubuntu/Debian
   sudo apt-get install python3.11 python3.11-pip
   ```

3. **Torrent Creation Tools** (at least one)
   - `torrenttools` (recommended, supports v1/v2/hybrid)
   - `mktorrent` (v1 only)
   - `transmission-create` (v1 only)

   ```bash
   # macOS
   brew install torrenttools mktorrent

   # Ubuntu/Debian
   sudo apt-get install mktorrent transmission-cli
   ```

### Installation

1. **Install Python dependencies:**
   ```bash
   cd integration_tests
   pip install -r requirements.txt
   ```

2. **Verify installation:**
   ```bash
   python run_tests.py --help
   ```

## Quick Start

Run all tests with a single command:

```bash
cd integration_tests
python run_tests.py
```

This will:
1. Clean up previous test data
2. Build the Superseedr Docker image
3. Start the Docker Compose environment
4. Execute all test scenarios (v1, v2, hybrid, seeding)
5. Report results
6. Clean up containers

## Running Tests Locally

### Run All Scenarios

```bash
python run_tests.py --verbose
```

### Run Specific Scenario

```bash
# v1 only
python run_tests.py --scenario v1

# v2 only
python run_tests.py --scenario v2

# Hybrid only
python run_tests.py --scenario hybrid

# Seeding only
python run_tests.py --scenario seeding
```

### Keep Containers Running (Debug Mode)

```bash
python run_tests.py --keep
```

Containers will remain running after tests complete. You can inspect them:

```bash
# View logs
docker compose logs superseedr
docker compose logs qbittorrent

# Access qBittorrent WebUI
open http://localhost:8080
# Default credentials: admin / adminadmin

# Stop containers when done
docker compose down
```

## Test Scenarios

### Scenario A: v1 Standard Download

Tests Superseedr downloading from qBittorrent using classic v1 torrent format.

**Steps:**
1. Generate v1 torrent with 1MB test data
2. Add to qBittorrent as seeder (skip hash check)
3. Copy torrent to Superseedr watch directory
4. Wait for Superseedr to download
5. Verify downloaded file hash matches source

**Validation:** SHA1 hash of downloaded file matches original

### Scenario B: v2 (BEP 52) Download

Tests v2 torrent support with Merkle tree verification.

**Steps:**
1. Generate v2-only torrent
2. Seed via qBittorrent
3. Download via Superseedr
4. Verify file integrity

**Validation:** Tests `V2Mapping` and `V2RootInfo` structures in `src/torrent_file/mod.rs`

### Scenario C: Hybrid Download

Tests hybrid torrents containing both v1 and v2 structures.

**Steps:**
1. Generate hybrid torrent
2. Seed via qBittorrent
3. Download via Superseedr
4. Verify backward compatibility

**Validation:** Ensures Superseedr correctly handles both protocol versions

### Scenario D: Seeding (Upload)

Tests Superseedr's ability to seed to other clients.

**Steps:**
1. Place complete file in Superseedr downloads directory
2. Add torrent to Superseedr watch directory
3. Add same torrent to qBittorrent as leecher
4. Monitor upload from Superseedr to qBittorrent
5. Verify qBittorrent completes download

**Validation:** Confirms data successfully transfers from Superseedr

## Architecture

### Directory Structure

```
integration_tests/
├── .github/
│   └── workflows/
│       └── integration-tests.yml    # CI/CD pipeline
├── config/
│   └── settings.toml                # Superseedr configuration
├── data/
│   ├── tracker_data/                # Tracker state
│   ├── superseedr_watch/            # Torrent watch directory
│   ├── superseedr_downloads/        # Download location
│   ├── superseedr_status/           # Status file output
│   └── reference_client_downloads/  # qBittorrent downloads
├── src/
│   ├── __init__.py
│   ├── torrent_generator.py         # Torrent creation utility
│   ├── superseedr_monitor.py        # Status monitoring
│   ├── qbittorrent_client.py        # API client wrapper
│   └── scenarios.py                 # Test implementations
├── docker-compose.yml               # Service orchestration
├── requirements.txt                 # Python dependencies
├── run_tests.py                     # Main test runner
└── README.md                        # This guide
```

### Key Components

#### Torrent Generator (`src/torrent_generator.py`)

Creates test torrents in v1, v2, or hybrid format using available CLI tools.

```python
from src.torrent_generator import TorrentGenerator, TorrentVersion

generator = TorrentGenerator("./test_output")
info = generator.create_test_torrent(
    name="my_test",
    version=TorrentVersion.V1,
    size=1048576,  # 1MB
    num_files=1
)
```

#### Superseedr Monitor (`src/superseedr_monitor.py`)

Polls the JSON status file generated by Superseedr.

```python
from src.superseedr_monitor import SuperseedrMonitor

monitor = SuperseedrMonitor("./data/superseedr_status")
status = monitor.read_status()

# Wait for torrent completion
success = monitor.wait_for_completion(
    info_hash="aabbccdd...",
    timeout=120
)
```

**Status File Location:** `data/superseedr_status/status_files/app_state.json`

**Example Status Output:**
```json
{
  "run_time": 45,
  "cpu_usage": 5.2,
  "ram_usage_percent": 12.8,
  "total_download_bps": 1048576,
  "total_upload_bps": 0,
  "torrents": {
    "aabbccdd1234...": {
      "torrent_name": "test_download",
      "progress": 0.75,
      "download_rate_bps": 1048576,
      "upload_rate_bps": 0,
      "num_peers": 1,
      "state": "downloading"
    }
  }
}
```

#### qBittorrent Client (`src/qbittorrent_client.py`)

Wraps the qBittorrent Web API for controlling the reference client.

```python
from src.qbittorrent_client import QBittorrentClient

client = QBittorrentClient("http://localhost:8080")
client.authenticate()

# Add torrent as seeder
client.add_torrent(
    torrent_path="./test.torrent",
    skip_checking=True
)

# Wait for seeding
client.wait_for_seeding(info_hash, timeout=30)
```

**qBittorrent WebUI:** http://localhost:8080  
**Default Credentials:** admin / adminadmin

## Troubleshooting

### Common Issues

#### 1. Torrent Creation Fails

**Error:** `RuntimeError: No torrent creation tool found`

**Solution:** Install a torrent creation tool:
```bash
# macOS
brew install torrenttools

# Ubuntu/Debian
sudo apt-get install mktorrent

# Or via pip
pip install torrenttools
```

#### 2. qBittorrent API Connection Failed

**Error:** Connection refused when accessing qBittorrent API

**Solution:** 
- Wait for qBittorrent to fully start (can take 10-20 seconds)
- Check container logs: `docker compose logs qbittorrent`
- Verify WebUI is enabled in qBittorrent settings

#### 3. Superseedr Status File Not Found

**Error:** `No status file found`

**Solution:**
- Check if Superseedr container is running: `docker compose ps`
- Verify settings.toml has `output_status_interval = 1`
- Check container logs: `docker compose logs superseedr`
- Check permissions on data directories

#### 4. Download Times Out

**Error:** `Timeout waiting for torrent ... to complete`

**Troubleshooting Steps:**
1. Check tracker is running:
   ```bash
   docker compose logs tracker
   ```

2. Verify peers can connect:
   ```bash
   docker compose exec qbittorrent curl http://tracker:6969/announce
   ```

3. Check Superseedr can see qBittorrent:
   ```bash
   docker compose exec superseedr ping qbittorrent
   ```

4. Review status file for error state:
   ```bash
   cat data/superseedr_status/status_files/app_state.json | jq
   ```

#### 5. Hash Verification Failed

**Error:** `File hash verification failed`

**Solution:**
- Check if download completed fully
- Verify no partial files exist (.part files)
- Check disk space and permissions
- Review logs for disk I/O errors

### Debug Mode

Enable verbose logging:

```bash
python run_tests.py --verbose --keep
```

Access running containers:

```bash
# Enter Superseedr container
docker compose exec superseedr sh

# Check status file
cat /app/status/status_files/app_state.json

# View logs in real-time
docker compose logs -f superseedr
```

### Collecting Artifacts

When tests fail, artifacts are automatically collected:

```bash
# Find artifact directory
ls -la artifacts/

# View logs
less artifacts/v1_failure/docker_logs.txt

# Analyze status
jq . artifacts/v1_failure/status_files/app_state.json
```

### Manual Inspection

While tests are running with `--keep` flag:

1. **Check qBittorrent WebUI:**
   - Open http://localhost:8080
   - Login: admin / adminadmin
   - Check torrent status, peers, trackers

2. **Check Tracker:**
   ```bash
   docker compose exec tracker wget -qO- http://localhost:6969/stats
   ```

3. **Monitor Network Traffic:**
   ```bash
   docker network inspect integration_tests_bittorrent-test
   ```

## Extending Tests

### Adding a New Scenario

1. **Create scenario class** in `src/scenarios.py`:

```python
class MyCustomScenario(TestScenario):
    def run(self) -> bool:
        logger.info("Running custom test...")
        
        # Generate torrent
        torrent_info = self.generator.create_test_torrent(...)
        
        # Add to reference client
        self.runner.qbittorrent.add_torrent(...)
        
        # Trigger Superseedr
        shutil.copy2(torrent_info.torrent_path, 
                     self.runner.superseedr_watch / "test.torrent")
        
        # Verify results
        return self.runner.superseedr.wait_for_completion(...)
```

2. **Register scenario** in `run_test_scenario()`:

```python
scenario_map = {
    "v1": V1DownloadScenario,
    "v2": V2DownloadScenario,
    "hybrid": HybridDownloadScenario,
    "seeding": SeedingScenario,
    "custom": MyCustomScenario,  # Add here
}
```

3. **Run new scenario**:

```bash
python run_tests.py --scenario custom
```

### Adding New Assertions

Use the base class methods in `TestScenario`:

```python
def verify_custom_metric(self, value: int) -> bool:
    """Custom verification logic."""
    if value < expected:
        logger.error(f"Metric too low: {value}")
        return False
    return True
```

### Modifying Test Parameters

Edit constants in scenario classes:

```python
class V1DownloadScenario(TestScenario):
    TEST_FILE_SIZE = 1048576  # 1MB, change as needed
    TIMEOUT = 120  # seconds
    NUM_FILES = 5  # Multi-file torrent
```

### CI/CD Integration

Tests run automatically on:
- Push to main/master/develop branches
- Pull requests modifying source code
- Manual workflow dispatch

Configure in `.github/workflows/integration-tests.yml`:

```yaml
on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday
```

## Additional Resources

- [qBittorrent Web API Documentation](https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-4.1))
- [BitTorrent Protocol Specification](http://bittorrent.org/beps/bep_0003.html)
- [BEP 52: BitTorrent v2](http://bittorrent.org/beps/bep_0052.html)

## Contributing

When adding new tests:

1. Follow existing code style
2. Add comprehensive logging
3. Include timeout mechanisms
4. Clean up resources in `teardown()`
5. Update this documentation

## Support

For issues with the integration test suite:

1. Check this troubleshooting guide
2. Review the [FAQ](../FAQ.md)
3. Open an issue on GitHub with:
   - Test scenario that failed
   - Container logs
   - Status file output
   - Your environment details (OS, Docker version, etc.)

---

**License:** GPL-3.0-or-later  
**Copyright:** 2025 The superseedr Contributors
