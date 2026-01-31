#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Integration Test Scenarios

Uses pre-generated torrent files for testing different BitTorrent protocol versions:
- Scenario A: v1 Standard Download (qBittorrent seeder -> Transmission downloader)
- Scenario B: v2 (BEP 52) Download  
- Scenario C: Hybrid Download
- Scenario D: Seeding (Upload) Test (Transmission seeding -> qBittorrent download)
"""

import hashlib
import logging
import shutil
import subprocess
import time
from pathlib import Path
from typing import Optional

logger = logging.getLogger(__name__)


class TestScenario:
    """Base class for test scenarios."""
    
    def __init__(self, runner):
        """
        Initialize test scenario.
        
        Args:
            runner: IntegrationTestRunner instance
        """
        self.runner = runner
        self.torrents_dir = self.runner.project_dir / "torrents"
    
    def run(self) -> bool:
        """Run the test scenario. Returns True if successful."""
        raise NotImplementedError
    
    def verify_file_hash(self, file_path: Path, expected_hash: str) -> bool:
        """Verify SHA1 hash of a downloaded file."""
        sha1 = hashlib.sha1()
        with open(file_path, "rb") as f:
            while chunk := f.read(8192):
                sha1.update(chunk)
        
        actual_hash = sha1.hexdigest()
        if actual_hash != expected_hash:
            logger.error(f"Hash mismatch for {file_path}")
            logger.error(f"  Expected: {expected_hash}")
            logger.error(f"  Actual:   {actual_hash}")
            return False
        
        return True
    
    def get_pregenerated_torrent(self, test_name: str) -> Optional["TorrentInfo"]:
        """Get a pre-generated torrent from the torrents directory."""
        torrent_path = self.torrents_dir / f"{test_name}.torrent"
        
        if torrent_path.exists():
            logger.info(f"  Using pre-generated torrent: {torrent_path}")
            return self._parse_torrent(torrent_path)
        
        logger.error(f"  Pre-generated torrent not found: {torrent_path}")
        return None
    
    def _parse_torrent(self, path: Path) -> "TorrentInfo":
        """Parse a torrent file to extract metadata."""
        import bencodepy
        
        with open(path, "rb") as f:
            data = bencodepy.decode(f.read())
        
        info = data.get(b"info", {})
        info_bencoded = bencodepy.encode(info)
        info_hash = hashlib.sha1(info_bencoded).hexdigest()
        
        files = []
        if b"length" in info:
            files.append((info.get(b"name", b"unknown").decode(), info[b"length"]))
        elif b"files" in info:
            for f in info[b"files"]:
                path_parts = [p.decode() for p in f[b"path"]]
                files.append(("/".join(path_parts), f[b"length"]))
        
        return TorrentInfo(
            torrent_path=path,
            info_hash=info_hash,
            data_path=path.parent,
            files=files
        )
    
    def get_test_data_dir(self) -> Optional[Path]:
        """Get the directory containing pre-generated test data."""
        test_data_dir = self.runner.data_dir / "test_data"
        
        if test_data_dir.exists():
            return test_data_dir
        
        return None
    
    def calculate_file_hash(self, file_path: Path) -> str:
        """Calculate SHA1 hash of a file."""
        sha1 = hashlib.sha1()
        with open(file_path, "rb") as f:
            while chunk := f.read(8192):
                sha1.update(chunk)
        return sha1.hexdigest()


class TorrentInfo:
    """Information about a torrent file."""
    
    def __init__(self, torrent_path: Path, info_hash: str, data_path: Path, files):
        self.torrent_path = torrent_path
        self.info_hash = info_hash
        self.data_path = data_path
        self.files = files


class V1DownloadScenario(TestScenario):
    """
    Scenario A: v1 Standard Download
    
    Tests that Transmission can download a v1 torrent from qBittorrent.
    """
    
    def run(self) -> bool:
        """Execute the v1 download test."""
        logger.info("=" * 60)
        logger.info("Scenario A: v1 Standard Download")
        logger.info("=" * 60)
        
        test_name = "v1_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            return False
        
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        expected_hash = None
        data_dir = self.get_test_data_dir()
        if data_dir and torrent_info.files:
            first_file = torrent_info.files[0][0]
            data_path = data_dir / first_file
            if data_path.exists():
                expected_hash = self.calculate_file_hash(data_path)
                logger.info(f"  Source SHA1: {expected_hash}")
        
        logger.info("\nStep 1: Copying test data to qBittorrent for seeding...")
        
        for filename, size in torrent_info.files:
            src_file = data_dir / filename if data_dir else None
            if src_file and src_file.exists():
                result = subprocess.run(
                    ["docker", "cp", str(src_file), f"qbittorrent-reference:/downloads/{filename}"],
                    capture_output=True,
                    text=True
                )
                if result.returncode == 0:
                    logger.info(f"  Copied {filename} to qBittorrent container")
                    # Fix ownership to match container user (abc:users = 1000:1000)
                    chown_result = subprocess.run(
                        ["docker", "exec", "qbittorrent-reference", "chown", "1000:1000", f"/downloads/{filename}"],
                        capture_output=True,
                        text=True
                    )
                    if chown_result.returncode != 0:
                        logger.warning(f"  Failed to fix ownership: {chown_result.stderr}")
                else:
                    logger.error(f"  Failed to copy {filename}: {result.stderr}")
                    return False
            else:
                logger.error(f"  Source data file not found: {src_file}")
                return False
        
        logger.info("\nStep 2: Adding torrent to qBittorrent as seeder...")
        self.runner.qbittorrent.wait_for_startup(timeout=60)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path="/downloads",
            skip_checking=True,
        )
        
        if not success:
            logger.error("Failed to add torrent to qBittorrent")
            return False
        
        logger.info("  Waiting for qBittorrent to start seeding...")
        seeding = self.runner.qbittorrent.wait_for_seeding(
            torrent_info.info_hash,
            timeout=30
        )
        
        if not seeding:
            logger.error("qBittorrent did not start seeding")
            return False
        
        logger.info("  qBittorrent is seeding")
        
        logger.info("\nStep 3: Adding torrent to Transmission as downloader...")
        self.runner.transmission.wait_for_startup(timeout=30)
        
        success = self.runner.transmission.add_torrent(
            torrent_path=torrent_info.torrent_path,
            download_dir="/downloads",
        )
        
        if not success:
            logger.error("Failed to add torrent to Transmission")
            return False
        
        logger.info("  Torrent added to Transmission")
        
        logger.info("\nStep 4: Waiting for Transmission to complete download...")
        completed = self.runner.transmission.wait_for_download(
            torrent_info.info_hash,
            timeout=120
        )
        
        if not completed:
            logger.error("Transmission did not complete download within timeout")
            return False
        
        logger.info("  Download completed!")
        
        logger.info("\nStep 5: Verifying downloaded file integrity...")
        downloaded_file = Path("/downloads") / torrent_info.files[0][0]
        
        result = subprocess.run(
            ["docker", "exec", "transmission-test", "test", "-f", str(downloaded_file)],
            capture_output=True,
            text=True
        )
        
        if result.returncode != 0:
            logger.error(f"Downloaded file not found: {downloaded_file}")
            return False
        
        if expected_hash:
            result = subprocess.run(
                ["docker", "exec", "transmission-test", "sha1sum", str(downloaded_file)],
                capture_output=True,
                text=True
            )
            actual_hash = result.stdout.split()[0] if result.returncode == 0 else None
            
            if actual_hash == expected_hash:
                logger.info("  File hash verified successfully")
                return True
            else:
                logger.error(f"  File hash mismatch: expected {expected_hash}, got {actual_hash}")
                return False
        else:
            logger.info("  Download completed (hash verification skipped)")
            return True


class V2DownloadScenario(TestScenario):
    """
    Scenario B: v2 (BEP 52) Download
    
    Tests that Transmission can correctly download a v2 torrent.
    """
    
    def run(self) -> bool:
        """Execute the v2 download test."""
        logger.info("=" * 60)
        logger.info("Scenario B: v2 (BEP 52) Download")
        logger.info("=" * 60)
        
        test_name = "v2_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            return False
        
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Copying test data to qBittorrent for seeding...")
        data_dir = self.get_test_data_dir()
        for filename, size in torrent_info.files:
            src_file = data_dir / filename if data_dir else None
            if src_file and src_file.exists():
                result = subprocess.run(
                    ["docker", "cp", str(src_file), f"qbittorrent-reference:/downloads/{filename}"],
                    capture_output=True,
                    text=True
                )
                if result.returncode == 0:
                    logger.info(f"  Copied {filename} to qBittorrent container")
                else:
                    logger.error(f"  Failed to copy {filename}")
                    return False
            else:
                logger.error(f"  Source data file not found: {src_file}")
                return False
        
        logger.info("\nStep 2: Adding torrent to qBittorrent...")
        self.runner.qbittorrent.wait_for_startup(timeout=30)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path="/downloads",
        )
        
        if not success:
            logger.error("Failed to add v2 torrent to qBittorrent")
            return False
        
        if not self.runner.qbittorrent.wait_for_seeding(
            torrent_info.info_hash, timeout=30
        ):
            logger.error("qBittorrent did not start seeding v2 torrent")
            return False
        
        logger.info("  qBittorrent is seeding v2 torrent")
        
        logger.info("\nStep 3: Adding torrent to Transmission...")
        self.runner.transmission.wait_for_startup(timeout=30)
        
        success = self.runner.transmission.add_torrent(
            torrent_path=torrent_info.torrent_path,
            download_dir="/downloads",
        )
        
        if not success:
            logger.error("Failed to add torrent to Transmission")
            return False
        
        logger.info("\nStep 4: Waiting for v2 download to complete...")
        if not self.runner.transmission.wait_for_download(
            torrent_info.info_hash, timeout=120
        ):
            logger.error("Transmission did not complete v2 download")
            return False
        
        logger.info("  v2 download completed!")
        
        logger.info("\nStep 5: Verifying downloaded file...")
        downloaded_file = Path("/downloads") / "test.bin"
        
        result = subprocess.run(
            ["docker", "exec", "transmission-test", "test", "-f", str(downloaded_file)],
            capture_output=True,
            text=True
        )
        
        if result.returncode == 0:
            logger.info("  v2 download verified successfully")
            return True
        else:
            logger.error("Downloaded file not found")
            return False


class HybridDownloadScenario(TestScenario):
    """
    Scenario C: Hybrid Download
    
    Tests that Transmission can handle hybrid torrents containing both
    v1 and v2 structures.
    """
    
    def run(self) -> bool:
        """Execute the hybrid download test."""
        logger.info("=" * 60)
        logger.info("Scenario C: Hybrid Torrent Download")
        logger.info("=" * 60)
        
        test_name = "hybrid_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            return False
        
        logger.info(f"  Info Hash (v1): {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Copying test data to qBittorrent for seeding...")
        data_dir = self.get_test_data_dir()
        for filename, size in torrent_info.files:
            src_file = data_dir / filename if data_dir else None
            if src_file and src_file.exists():
                result = subprocess.run(
                    ["docker", "cp", str(src_file), f"qbittorrent-reference:/downloads/{filename}"],
                    capture_output=True,
                    text=True
                )
                if result.returncode == 0:
                    logger.info(f"  Copied {filename} to qBittorrent container")
                else:
                    logger.error(f"  Failed to copy {filename}")
                    return False
            else:
                logger.error(f"  Source data file not found: {src_file}")
                return False
        
        logger.info("\nStep 2: Adding torrent to qBittorrent...")
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path="/downloads",
        )
        
        if not success:
            logger.error("Failed to add hybrid torrent")
            return False
        
        if not self.runner.qbittorrent.wait_for_seeding(
            torrent_info.info_hash, timeout=30
        ):
            logger.error("qBittorrent did not start seeding")
            return False
        
        logger.info("  qBittorrent is seeding hybrid torrent")
        
        logger.info("\nStep 3: Adding torrent to Transmission...")
        self.runner.transmission.wait_for_startup(timeout=30)
        
        success = self.runner.transmission.add_torrent(
            torrent_path=torrent_info.torrent_path,
            download_dir="/downloads",
        )
        
        if not success:
            logger.error("Failed to add torrent to Transmission")
            return False
        
        logger.info("\nStep 4: Waiting for hybrid download to complete...")
        if not self.runner.transmission.wait_for_download(
            torrent_info.info_hash, timeout=120
        ):
            logger.error("Download did not complete")
            return False
        
        logger.info("  Hybrid download completed!")
        
        logger.info("\nStep 5: Verifying downloaded file...")
        downloaded_file = Path("/downloads") / torrent_info.files[0][0]
        
        result = subprocess.run(
            ["docker", "exec", "transmission-test", "test", "-f", str(downloaded_file)],
            capture_output=True,
            text=True
        )
        
        if result.returncode == 0:
            logger.info("  Hybrid torrent download verified")
            return True
        else:
            logger.error("Downloaded file not found")
            return False


class SeedingScenario(TestScenario):
    """
    Scenario D: Seeding (Upload) Test
    
    Tests that Transmission can seed files to qBittorrent.
    """
    
    def run(self) -> bool:
        """Execute the seeding test."""
        logger.info("=" * 60)
        logger.info("Scenario D: Seeding (Upload) Test")
        logger.info("=" * 60)
        
        test_name = "v1_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            return False
        
        logger.info(f"  Torrent: {torrent_info.torrent_path}")
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Checking for data file in Transmission downloads...")
        
        if not torrent_info.files:
            logger.error("  No files found in torrent")
            return False
        
        source_filename = torrent_info.files[0][0]
        data_dir = self.get_test_data_dir()
        source_file = data_dir / source_filename if data_dir else None
        
        if not source_file or not source_file.exists():
            logger.error(f"  Data file not found: {source_file}")
            logger.info("  Place the data file in data/test_data/ before running test")
            return False
        
        logger.info(f"  Found data file: {source_file}")
        
        logger.info("\nStep 2: Copying data to Transmission container...")
        
        result = subprocess.run(
            ["docker", "cp", str(source_file), "transmission-test:/downloads/"],
            capture_output=True,
            text=True
        )
        if result.returncode != 0:
            logger.error(f"  Failed to copy data: {result.stderr}")
            return False
        
        logger.info(f"  Copied {source_filename} to Transmission")
        
        logger.info("\nStep 3: Adding torrent to Transmission...")
        self.runner.transmission.wait_for_startup(timeout=30)
        
        success = self.runner.transmission.add_torrent(
            torrent_path=torrent_info.torrent_path,
            download_dir="/downloads",
            paused=True,
        )
        
        if not success:
            logger.error("Failed to add torrent to Transmission")
            return False
        
        logger.info("  Torrent added to Transmission")
        
        logger.info("\nStep 4: Starting Transmission torrent...")
        if not self.runner.transmission.start_torrent(torrent_info.info_hash):
            logger.error("Failed to start Transmission torrent")
            return False
        
        logger.info("\nStep 5: Adding torrent to qBittorrent in download mode...")
        
        self.runner.qbittorrent.wait_for_startup(timeout=30)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path="/downloads",
        )
        
        if not success:
            logger.error("Failed to add torrent to qBittorrent")
            return False
        
        logger.info("\nStep 6: Monitoring data transfer from Transmission to qBittorrent...")
        
        start_time = time.time()
        max_wait = 120
        upload_detected = False
        download_completed = False
        
        while time.time() - start_time < max_wait:
            torrent = self.runner.qbittorrent.get_torrent(torrent_info.info_hash)
            if torrent:
                logger.debug(
                    f"  qBittorrent: {torrent.state}, "
                    f"Progress: {torrent.progress*100:.1f}%, "
                    f"DL: {torrent.dlspeed} B/s"
                )
                
                if torrent.dlspeed > 0:
                    upload_detected = True
                
                if torrent.progress >= 1.0:
                    download_completed = True
                    break
            
            time.sleep(2)
        
        logger.info("\nStep 7: Verifying upload results...")
        
        if not upload_detected:
            logger.error("  No upload detected from Transmission")
            return False
        
        logger.info("  Upload activity detected from Transmission")
        
        if not download_completed:
            logger.error("  qBittorrent did not complete download")
            return False
        
        logger.info("  qBittorrent completed download from Transmission")
        
        return True


def run_test_scenario(runner, scenario_name: str) -> bool:
    """
    Run a specific test scenario.
    
    Args:
        runner: IntegrationTestRunner instance
        scenario_name: Name of the scenario to run (v1, v2, hybrid, seeding)
        
    Returns:
        True if scenario passed
    """
    scenario_map = {
        "v1": V1DownloadScenario,
        "v2": V2DownloadScenario,
        "hybrid": HybridDownloadScenario,
        "seeding": SeedingScenario,
    }
    
    if scenario_name not in scenario_map:
        raise ValueError(f"Unknown scenario: {scenario_name}")
    
    scenario_class = scenario_map[scenario_name]
    scenario = scenario_class(runner)
    
    try:
        return scenario.run()
    except Exception as e:
        logger.exception(f"Scenario {scenario_name} failed with exception")
        return False


if __name__ == "__main__":
    logging.basicConfig(level=logging.DEBUG)
    
    print("Scenarios module loaded. Use run_tests.py to execute tests.")
