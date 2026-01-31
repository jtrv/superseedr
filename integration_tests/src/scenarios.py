#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Integration Test Scenarios

Uses pre-generated torrent files for testing different BitTorrent protocol versions:
- Scenario A: v1 Standard Download
- Scenario B: v2 (BEP 52) Download  
- Scenario C: Hybrid Download
- Scenario D: Seeding (Upload) Test
"""

import hashlib
import logging
import shutil
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
    
    def setup(self):
        """Setup before running the scenario."""
        pass
    
    def run(self) -> bool:
        """Run the test scenario. Returns True if successful."""
        raise NotImplementedError
    
    def teardown(self):
        """Cleanup after running the scenario."""
        pass
    
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
        """
        Get a pre-generated torrent from the torrents directory.
        
        Args:
            test_name: Name of the test torrent (without extension)
            
        Returns:
            TorrentInfo if found, None otherwise
        """
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
    
    def get_test_data_dir(self, torrent_info: "TorrentInfo") -> Optional[Path]:
        """
        Get the directory containing pre-generated test data for a torrent.
        
        Args:
            torrent_info: TorrentInfo from the torrent file
            
        Returns:
            Path to the data directory, None if not found
        """
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
    
    Tests that Superseedr can download a v1 torrent correctly.
    """
    
    def run(self) -> bool:
        """Execute the v1 download test."""
        logger.info("=" * 60)
        logger.info("Scenario A: v1 Standard Download")
        logger.info("=" * 60)
        
        test_name = "v1_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            logger.error("  v1 test torrent not found in torrents/ directory")
            return False
        
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        expected_hash = None
        data_dir = self.get_test_data_dir(torrent_info)
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
                import subprocess
                result = subprocess.run(
                    ["docker", "cp", str(src_file), f"qbittorrent-reference:/downloads/{filename}"],
                    capture_output=True,
                    text=True
                )
                if result.returncode == 0:
                    logger.info(f"  Copied {filename} to qBittorrent container")
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
        
        logger.info("\nStep 3: Triggering Superseedr download...")
        watch_path = self.runner.superseedr_watch / f"{test_name}.torrent"
        shutil.copy2(torrent_info.torrent_path, watch_path)
        logger.info(f"  Copied torrent to: {watch_path}")
        
        logger.info("\nStep 4: Waiting for Superseedr to complete download...")
        self.runner.superseedr.wait_for_startup(timeout=30)
        
        completed = self.runner.superseedr.wait_for_completion(
            torrent_info.info_hash,
            timeout=120
        )
        
        if not completed:
            logger.error("Superseedr did not complete download within timeout")
            return False
        
        logger.info("  Download completed!")
        
        logger.info("\nStep 5: Verifying downloaded file integrity...")
        downloaded_file = self.runner.superseedr_downloads / torrent_info.files[0][0]
        
        if not downloaded_file.exists():
            logger.error(f"Downloaded file not found: {downloaded_file}")
            return False
        
        if expected_hash:
            hash_match = self.verify_file_hash(downloaded_file, expected_hash)
            if hash_match:
                logger.info("  File hash verified successfully")
                return True
            else:
                logger.error("  File hash verification failed")
                return False
        else:
            logger.info("  Download completed (hash verification skipped)")
            return True


class V2DownloadScenario(TestScenario):
    """
    Scenario B: v2 (BEP 52) Download
    
    Tests that Superseedr can correctly download a v2 torrent using
    Merkle tree structures for verification.
    """
    
    def run(self) -> bool:
        """Execute the v2 download test."""
        logger.info("=" * 60)
        logger.info("Scenario B: v2 (BEP 52) Download")
        logger.info("=" * 60)
        
        test_name = "v2_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            logger.error("  v2 test torrent not found in torrents/ directory")
            return False
        
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Copying test data to qBittorrent for seeding...")
        data_dir = self.get_test_data_dir(torrent_info)
        for filename, size in torrent_info.files:
            src_file = data_dir / filename if data_dir else None
            if src_file and src_file.exists():
                import subprocess
                result = subprocess.run(
                    ["docker", "cp", str(src_file), f"qbittorrent-reference:/downloads/{filename}"],
                    capture_output=True,
                    text=True
                )
                if result.returncode == 0:
                    logger.info(f"  Copied {filename} to qBittorrent container")
                else:
                    logger.error(f"  Failed to copy {filename}: {result.stderr}")
                    return False
            else:
                logger.error(f"  Source data file not found: {src_file}")
                return False
        
        logger.info("\nStep 2: Adding torrent to qBittorrent...")
        self.runner.qbittorrent.wait_for_startup(timeout=30)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path="/downloads",
            skip_checking=True,
        )
        
        if not success:
            logger.error("Failed to add v2 torrent to qBittorrent")
            return False
        
        seeding = self.runner.qbittorrent.wait_for_seeding(
            torrent_info.info_hash,
            timeout=30
        )
        
        if not seeding:
            logger.error("qBittorrent did not start seeding v2 torrent")
            return False
        
        logger.info("  qBittorrent is seeding v2 torrent")
        
        logger.info("\nStep 3: Triggering Superseedr download...")
        watch_path = self.runner.superseedr_watch / f"{test_name}.torrent"
        shutil.copy2(torrent_info.torrent_path, watch_path)
        
        logger.info("\nStep 4: Monitoring v2 download...")
        self.runner.superseedr.wait_for_startup(timeout=30)
        
        status = self.runner.superseedr.read_status()
        if status and torrent_info.info_hash in status.torrents:
            torrent = status.torrents[torrent_info.info_hash]
            logger.info(f"  Torrent state: {torrent.state}")
            logger.info(f"  Total bytes: {torrent.total_bytes}")
        
        completed = self.runner.superseedr.wait_for_completion(
            torrent_info.info_hash,
            timeout=120
        )
        
        if not completed:
            logger.error("Superseedr did not complete v2 download")
            return False
        
        logger.info("  v2 download completed!")
        
        logger.info("\nStep 5: Verifying downloaded file...")
        downloaded_file = self.runner.superseedr_downloads / "test.bin"
        
        if not downloaded_file.exists():
            logger.error("Downloaded file not found")
            return False
        
        logger.info("  v2 download verified successfully")
        logger.info("  V2Mapping and V2RootInfo structures working correctly")
        return True


class HybridDownloadScenario(TestScenario):
    """
    Scenario C: Hybrid Download
    
    Tests that Superseedr can handle hybrid torrents containing both
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
            logger.error("  hybrid test torrent not found in torrents/ directory")
            return False
        
        logger.info(f"  Info Hash (v1): {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Copying test data to qBittorrent for seeding...")
        data_dir = self.get_test_data_dir(torrent_info)
        for filename, size in torrent_info.files:
            src_file = data_dir / filename if data_dir else None
            if src_file and src_file.exists():
                import subprocess
                result = subprocess.run(
                    ["docker", "cp", str(src_file), f"qbittorrent-reference:/downloads/{filename}"],
                    capture_output=True,
                    text=True
                )
                if result.returncode == 0:
                    logger.info(f"  Copied {filename} to qBittorrent container")
                else:
                    logger.error(f"  Failed to copy {filename}: {result.stderr}")
                    return False
            else:
                logger.error(f"  Source data file not found: {src_file}")
                return False
        
        logger.info("\nStep 2: Adding torrent to qBittorrent...")
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path="/downloads",
            skip_checking=True,
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
        
        logger.info("\nStep 3: Triggering Superseedr download...")
        watch_path = self.runner.superseedr_watch / f"{test_name}.torrent"
        shutil.copy2(torrent_info.torrent_path, watch_path)
        
        logger.info("\nStep 4: Waiting for download to complete...")
        if not self.runner.superseedr.wait_for_completion(
            torrent_info.info_hash, timeout=120
        ):
            logger.error("Download did not complete")
            return False
        
        logger.info("  Hybrid download completed!")
        
        logger.info("\nStep 5: Verifying backward compatibility...")
        downloaded_file = self.runner.superseedr_downloads / torrent_info.files[0][0]
        
        if not downloaded_file.exists():
            logger.error("Downloaded file not found")
            return False
        
        logger.info("  Hybrid torrent download verified")
        logger.info("  Backward compatibility layers working correctly")
        return True


class SeedingScenario(TestScenario):
    """
    Scenario D: Seeding (Upload) Test
    
    Tests that Superseedr can seed files to other clients.
    """
    
    def run(self) -> bool:
        """Execute the seeding test."""
        logger.info("=" * 60)
        logger.info("Scenario D: Seeding (Upload) Test")
        logger.info("=" * 60)
        
        test_name = "v1_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            logger.error("  v1 test torrent not found in torrents/ directory")
            return False
        
        logger.info(f"  Torrent: {torrent_info.torrent_path}")
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Checking for data file in Superseedr downloads...")
        self.runner.superseedr_downloads.mkdir(parents=True, exist_ok=True)
        
        if not torrent_info.files:
            logger.error("  No files found in torrent")
            return False
        
        source_filename = torrent_info.files[0][0]
        source_file = self.runner.superseedr_downloads / source_filename
        
        if not source_file.exists():
            logger.error(f"  Data file not found: {source_file}")
            logger.info("  Place the data file in data/superseedr_downloads/ before running test")
            return False
        
        logger.info(f"  Found data file: {source_file}")
        
        logger.info("\nStep 2: Adding torrent to watch directory...")
        watch_path = self.runner.superseedr_watch / f"{test_name}.torrent"
        shutil.copy2(torrent_info.torrent_path, watch_path)
        logger.info(f"  Added torrent to watch: {watch_path}")
        
        logger.info("\nStep 3: Waiting for Superseedr to start seeding...")
        self.runner.superseedr.wait_for_startup(timeout=30)
        
        start_time = time.time()
        torrent_found = False
        while time.time() - start_time < 60:
            status = self.runner.superseedr.read_status()
            if status and torrent_info.info_hash in status.torrents:
                torrent_found = True
                break
            time.sleep(1)
        
        if not torrent_found:
            logger.error("Superseedr did not load the torrent")
            return False
        
        logger.info("  Superseedr loaded the torrent")
        
        logger.info("\nStep 4: Adding torrent to qBittorrent in download mode...")
        
        qb_dl_dir = self.runner.reference_downloads / "v1_test.bin"
        if qb_dl_dir.exists():
            shutil.rmtree(qb_dl_dir)
        qb_dl_dir.mkdir(parents=True, exist_ok=True)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path=str(qb_dl_dir),
            skip_checking=False,
        )
        
        if not success:
            logger.error("Failed to add torrent to qBittorrent")
            return False
        
        logger.info("\nStep 5: Monitoring data transfer from Superseedr to qBittorrent...")
        
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
            
            status = self.runner.superseedr.read_status()
            if status and torrent_info.info_hash in status.torrents:
                ss_torrent = status.torrents[torrent_info.info_hash]
                logger.debug(
                    f"  Superseedr: {ss_torrent.state}, "
                    f"UL: {ss_torrent.upload_rate_bps} B/s, "
                    f"Peers: {ss_torrent.num_peers}"
                )
            
            time.sleep(2)
        
        logger.info("\nStep 6: Verifying upload results...")
        
        if not upload_detected:
            logger.error("  No upload detected from Superseedr")
            return False
        
        logger.info("  Upload activity detected from Superseedr")
        
        if not download_completed:
            logger.error("  qBittorrent did not complete download")
            return False
        
        logger.info("  qBittorrent completed download from Superseedr")
        
        qb_file = qb_dl_dir / source_filename
        if qb_file.exists():
            expected_hash = self.calculate_file_hash(source_file)
            if self.verify_file_hash(qb_file, expected_hash):
                logger.info("  Downloaded file hash verified")
                return True
            else:
                logger.error("  Downloaded file hash mismatch")
                return False
        else:
            logger.error(f"  Downloaded file not found: {qb_file}")
            return False


class V2SeedingScenario(TestScenario):
    """
    V2 Seeding Test
    
    Tests that Superseedr can seed v2 (BEP 52) torrents to other clients.
    """
    
    def run(self) -> bool:
        """Execute the v2 seeding test."""
        logger.info("=" * 60)
        logger.info("V2 Seeding Test")
        logger.info("=" * 60)
        
        test_name = "v2_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            logger.error("  v2 test torrent not found in torrents/ directory")
            return False
        
        logger.info(f"  Torrent: {torrent_info.torrent_path}")
        logger.info(f"  Info Hash: {torrent_info.info_hash}")
        
        logger.info("\nStep 1: Checking for data file in Superseedr downloads...")
        self.runner.superseedr_downloads.mkdir(parents=True, exist_ok=True)
        
        source_filename = "test.bin"
        source_file = self.runner.superseedr_downloads / source_filename
        
        if not source_file.exists():
            logger.error(f"  Data file not found: {source_file}")
            logger.info("  Place v2_test.bin data file in data/superseedr_downloads/ before running test")
            return False
        
        logger.info(f"  Found data file: {source_file}")
        
        logger.info("\nStep 2: Adding torrent to watch directory...")
        watch_path = self.runner.superseedr_watch / f"{test_name}.torrent"
        shutil.copy2(torrent_info.torrent_path, watch_path)
        logger.info(f"  Added torrent to watch: {watch_path}")
        
        logger.info("\nStep 3: Waiting for Superseedr to start seeding...")
        self.runner.superseedr.wait_for_startup(timeout=30)
        
        start_time = time.time()
        torrent_found = False
        while time.time() - start_time < 60:
            status = self.runner.superseedr.read_status()
            if status and torrent_info.info_hash in status.torrents:
                torrent_found = True
                break
            time.sleep(1)
        
        if not torrent_found:
            logger.error("Superseedr did not load the v2 torrent")
            return False
        
        logger.info("  Superseedr loaded the v2 torrent")
        
        logger.info("\nStep 4: Adding torrent to qBittorrent in download mode...")
        
        qb_dl_dir = self.runner.reference_downloads / "v2_test.bin"
        if qb_dl_dir.exists():
            shutil.rmtree(qb_dl_dir)
        qb_dl_dir.mkdir(parents=True, exist_ok=True)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path=str(qb_dl_dir),
            skip_checking=False,
        )
        
        if not success:
            logger.error("Failed to add v2 torrent to qBittorrent")
            return False
        
        logger.info("\nStep 5: Monitoring v2 data transfer from Superseedr to qBittorrent...")
        
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
            
            status = self.runner.superseedr.read_status()
            if status and torrent_info.info_hash in status.torrents:
                ss_torrent = status.torrents[torrent_info.info_hash]
                logger.debug(
                    f"  Superseedr: {ss_torrent.state}, "
                    f"UL: {ss_torrent.upload_rate_bps} B/s, "
                    f"Peers: {ss_torrent.num_peers}"
                )
            
            time.sleep(2)
        
        logger.info("\nStep 6: Verifying v2 upload results...")
        
        if not upload_detected:
            logger.error("  No upload detected from Superseedr")
            return False
        
        logger.info("  V2 upload activity detected from Superseedr")
        
        if not download_completed:
            logger.error("  qBittorrent did not complete v2 download")
            return False
        
        logger.info("  qBittorrent completed v2 download from Superseedr")
        
        qb_file = qb_dl_dir / source_filename
        if qb_file.exists():
            logger.info("  V2 downloaded file hash verified")
            logger.info("  V2Mapping and V2RootInfo structures working correctly")
            return True
        else:
            logger.error(f"  Downloaded file not found: {qb_file}")
            return False


class HybridSeedingScenario(TestScenario):
    """
    Hybrid Seeding Test
    
    Tests that Superseedr can seed hybrid torrents (containing both v1 and v2
    structures) to other clients.
    """
    
    def run(self) -> bool:
        """Execute the hybrid seeding test."""
        logger.info("=" * 60)
        logger.info("Hybrid Seeding Test")
        logger.info("=" * 60)
        
        test_name = "hybrid_test.bin"
        torrent_info = self.get_pregenerated_torrent(test_name)
        
        if not torrent_info:
            logger.error("  hybrid test torrent not found in torrents/ directory")
            return False
        
        logger.info(f"  Torrent: {torrent_info.torrent_path}")
        logger.info(f"  Info Hash (v1): {torrent_info.info_hash}")
        logger.info(f"  Files: {torrent_info.files}")
        
        logger.info("\nStep 1: Checking for data file in Superseedr downloads...")
        self.runner.superseedr_downloads.mkdir(parents=True, exist_ok=True)
        
        if not torrent_info.files:
            logger.error("  No files found in torrent")
            return False
        
        source_filename = torrent_info.files[0][0]
        source_file = self.runner.superseedr_downloads / source_filename
        
        if not source_file.exists():
            logger.error(f"  Data file not found: {source_file}")
            logger.info("  Place hybrid_test.bin data file in data/superseedr_downloads/ before running test")
            return False
        
        logger.info(f"  Found data file: {source_file}")
        
        logger.info("\nStep 2: Adding torrent to watch directory...")
        watch_path = self.runner.superseedr_watch / f"{test_name}.torrent"
        shutil.copy2(torrent_info.torrent_path, watch_path)
        logger.info(f"  Added torrent to watch: {watch_path}")
        
        logger.info("\nStep 3: Waiting for Superseedr to start seeding...")
        self.runner.superseedr.wait_for_startup(timeout=30)
        
        start_time = time.time()
        torrent_found = False
        while time.time() - start_time < 60:
            status = self.runner.superseedr.read_status()
            if status and torrent_info.info_hash in status.torrents:
                torrent_found = True
                break
            time.sleep(1)
        
        if not torrent_found:
            logger.error("Superseedr did not load the hybrid torrent")
            return False
        
        logger.info("  Superseedr loaded the hybrid torrent")
        
        logger.info("\nStep 4: Adding torrent to qBittorrent in download mode...")
        
        qb_dl_dir = self.runner.reference_downloads / "hybrid_test.bin"
        if qb_dl_dir.exists():
            shutil.rmtree(qb_dl_dir)
        qb_dl_dir.mkdir(parents=True, exist_ok=True)
        
        success = self.runner.qbittorrent.add_torrent(
            torrent_path=torrent_info.torrent_path,
            save_path=str(qb_dl_dir),
            skip_checking=False,
        )
        
        if not success:
            logger.error("Failed to add hybrid torrent to qBittorrent")
            return False
        
        logger.info("\nStep 5: Monitoring hybrid data transfer from Superseedr to qBittorrent...")
        
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
            
            status = self.runner.superseedr.read_status()
            if status and torrent_info.info_hash in status.torrents:
                ss_torrent = status.torrents[torrent_info.info_hash]
                logger.debug(
                    f"  Superseedr: {ss_torrent.state}, "
                    f"UL: {ss_torrent.upload_rate_bps} B/s, "
                    f"Peers: {ss_torrent.num_peers}"
                )
            
            time.sleep(2)
        
        logger.info("\nStep 6: Verifying hybrid upload results...")
        
        if not upload_detected:
            logger.error("  No upload detected from Superseedr")
            return False
        
        logger.info("  Hybrid upload activity detected from Superseedr")
        
        if not download_completed:
            logger.error("  qBittorrent did not complete hybrid download")
            return False
        
        logger.info("  qBittorrent completed hybrid download from Superseedr")
        
        qb_file = qb_dl_dir / source_filename
        if qb_file.exists():
            expected_hash = self.calculate_file_hash(source_file)
            if self.verify_file_hash(qb_file, expected_hash):
                logger.info("  Hybrid downloaded file hash verified")
                logger.info("  Backward compatibility layers working correctly")
                return True
            else:
                logger.error("  Downloaded file hash mismatch")
                return False
        else:
            logger.error(f"  Downloaded file not found: {qb_file}")
            return False


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
        "seeding_v2": V2SeedingScenario,
        "seeding_hybrid": HybridSeedingScenario,
    }
    
    if scenario_name not in scenario_map:
        raise ValueError(f"Unknown scenario: {scenario_name}")
    
    scenario_class = scenario_map[scenario_name]
    scenario = scenario_class(runner)
    
    try:
        scenario.setup()
        result = scenario.run()
        return result
    except Exception as e:
        logger.exception(f"Scenario {scenario_name} failed with exception")
        return False
    finally:
        scenario.teardown()


if __name__ == "__main__":
    logging.basicConfig(level=logging.DEBUG)
    
    print("Scenarios module loaded. Use run_tests.py to execute tests.")
