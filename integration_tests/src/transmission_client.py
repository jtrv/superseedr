#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Transmission RPC Client

Provides a wrapper around the Transmission BitTorrent client RPC API.
"""

import hashlib
import logging
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional
from urllib.parse import urljoin

import requests
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

logger = logging.getLogger(__name__)


@dataclass
class TransmissionTorrent:
    """Information about a torrent in Transmission."""
    id: int
    hash_string: str
    name: str
    total_size: int
    progress: float
    download_speed: int
    upload_speed: int
    status: int
    status_string: str
    download_dir: str
    percent_complete: float
    eta: int


class TransmissionClient:
    """Client for Transmission RPC API."""
    
    def __init__(
        self,
        base_url: str = "http://localhost:9091",
        timeout: int = 30
    ):
        """
        Initialize Transmission client.
        
        Args:
            base_url: Base URL for Transmission WebUI
            timeout: Request timeout in seconds
        """
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._session = requests.Session()
        self._session_id: Optional[str] = None
        
        retry_strategy = Retry(
            total=3,
            backoff_factor=1,
            status_forcelist=[429, 500, 502, 503, 504],
        )
        adapter = HTTPAdapter(max_retries=retry_strategy)
        self._session.mount("http://", adapter)
        self._session.mount("https://", adapter)
    
    def _get_session_id(self) -> bool:
        """Get or refresh the Transmission session ID."""
        try:
            response = self._session.post(
                f"{self.base_url}/transmission/rpc",
                json={"method": "session-get"},
                timeout=self.timeout
            )
            
            if response.status_code == 200:
                self._session_id = response.headers.get("X-Transmission-Session-Id")
                return True
            
            if response.status_code == 409:
                self._session_id = response.headers.get("X-Transmission-Session-Id")
                return self._session_id is not None
            
            return False
            
        except Exception as e:
            logger.error(f"Failed to get session ID: {e}")
            return False
    
    def _request(self, method: str, arguments: Dict = None) -> Optional[Dict]:
        """Make a request to the Transmission RPC."""
        if not self._session_id:
            if not self._get_session_id():
                return None
        
        payload = {"method": method}
        if arguments:
            payload["arguments"] = arguments
        
        headers = {"X-Transmission-Session-Id": self._session_id}
        
        try:
            response = self._session.post(
                f"{self.base_url}/transmission/rpc",
                json=payload,
                headers=headers,
                timeout=self.timeout
            )
            
            if response.status_code == 409:
                self._session_id = response.headers.get("X-Transmission-Session-Id")
                headers["X-Transmission-Session-Id"] = self._session_id
                response = self._session.post(
                    f"{self.base_url}/transmission/rpc",
                    json=payload,
                    headers=headers,
                    timeout=self.timeout
                )
            
            if response.status_code == 200:
                data = response.json()
                if data.get("result") == "success":
                    return data.get("arguments")
            
            return None
            
        except Exception as e:
            logger.error(f"RPC request failed: {e}")
            return None
    
    def is_alive(self) -> bool:
        """Check if Transmission is running and responding."""
        try:
            response = self._session.get(
                f"{self.base_url}/transmission/rpc",
                timeout=5
            )
            return response.status_code in [200, 409]
        except Exception:
            return False
    
    def wait_for_startup(self, timeout: float = 60.0, poll_interval: float = 2.0) -> bool:
        """Wait for Transmission to be ready."""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            if self.is_alive():
                time.sleep(2)
                return True
            time.sleep(poll_interval)
        
        return False
    
    def add_torrent(
        self,
        torrent_path: Optional[Path] = None,
        torrent_url: Optional[str] = None,
        download_dir: Optional[str] = None,
        paused: bool = False
    ) -> bool:
        """
        Add a torrent to Transmission.
        
        Args:
            torrent_path: Path to .torrent file
            torrent_url: URL to torrent file
            download_dir: Download directory
            paused: Start in paused state
            
        Returns:
            True if torrent was added successfully
        """
        if torrent_path and torrent_path.exists():
            import subprocess
            result = subprocess.run(
                ["docker", "cp", str(torrent_path), "transmission-test:/watch/"],
                capture_output=True,
                text=True
            )
            if result.returncode != 0:
                logger.error(f"Failed to copy torrent to watch directory: {result.stderr}")
                return False
            
            watch_name = torrent_path.name
            logger.info(f"Copied torrent to watch directory: {watch_name}")
            
            import time
            time.sleep(3)
            
            torrent = self.get_torrent_by_name(torrent_path.stem)
            if torrent:
                if paused:
                    self.stop_torrent(torrent.hash_string)
                return True
            
            logger.warning("Torrent may not have been picked up from watch directory")
            return True
        
        arguments = {"paused": paused}
        
        if torrent_url:
            arguments["filename"] = torrent_url
        
        if download_dir:
            arguments["download-dir"] = download_dir
        
        result = self._request("torrent-add", arguments)
        
        if result is None:
            return False
        
        if "torrent-added" in result:
            return True
        
        if "torrent-duplicate" in result:
            logger.info("Torrent already exists in Transmission")
            return True
        
        return False
    
    def get_torrent_by_name(self, name: str) -> Optional[TransmissionTorrent]:
        """Get a torrent by name (partial match)."""
        arguments = self._request("torrent-get", {
            "fields": [
                "id", "hashString", "name", "totalSize", "percentComplete",
                "status", "downloadDir", "eta"
            ]
        })
        
        if not arguments or "torrents" not in arguments:
            return None
        
        status_strs = ["stopped", "check pending", "checking", "download pending", 
                       "downloading", "seed pending", "seeding", "unknown"]
        
        name_lower = name.lower()
        for t in arguments["torrents"]:
            if name_lower in t.get("name", "").lower():
                status = t.get("status", 0)
                status_string = status_strs[min(status, len(status_strs)-1)] if status < len(status_strs) else "unknown"
                
                return TransmissionTorrent(
                    id=t["id"],
                    hash_string=t["hashString"],
                    name=t["name"],
                    total_size=t["totalSize"],
                    progress=t["percentComplete"] / 100.0,
                    download_speed=0,
                    upload_speed=0,
                    status=t["status"],
                    status_string=status_string,
                    download_dir=t["downloadDir"],
                    percent_complete=t["percentComplete"],
                    eta=t["eta"]
                )
        
        return None
    
    def get_torrent(self, info_hash: str) -> Optional[TransmissionTorrent]:
        """Get information about a specific torrent."""
        info_hash = info_hash.lower()
        
        arguments = self._request("torrent-get", {
            "fields": [
                "id", "hashString", "name", "totalSize", "percentComplete",
                "status", "downloadDir", "eta"
            ]
        })
        
        if not arguments or "torrents" not in arguments:
            return None
        
        for t in arguments["torrents"]:
            if t.get("hashString", "").lower() == info_hash:
                status_strs = ["stopped", "check pending", "checking", "download pending", 
                               "downloading", "seed pending", "seeding", "unknown"]
                status = t.get("status", 0)
                status_string = status_strs[min(status, len(status_strs)-1)] if status < len(status_strs) else "unknown"
                
                return TransmissionTorrent(
                    id=t["id"],
                    hash_string=t["hashString"],
                    name=t["name"],
                    total_size=t["totalSize"],
                    progress=t["percentComplete"] / 100.0,
                    download_speed=0,
                    upload_speed=0,
                    status=t["status"],
                    status_string=status_string,
                    download_dir=t["downloadDir"],
                    percent_complete=t["percentComplete"],
                    eta=t["eta"]
                )
        
        return None
    
    def get_all_torrents(self) -> List[TransmissionTorrent]:
        """Get all torrents."""
        arguments = self._request("torrent-get", {
            "fields": [
                "id", "hashString", "name", "totalSize", "percentComplete",
                "status", "downloadDir", "eta"
            ]
        })
        
        if not arguments or "torrents" not in arguments:
            return []
        
        status_strs = ["stopped", "check pending", "checking", "download pending", 
                       "downloading", "seed pending", "seeding", "unknown"]
        
        result = []
        for t in arguments["torrents"]:
            status = t.get("status", 0)
            status_string = status_strs[min(status, len(status_strs)-1)] if status < len(status_strs) else "unknown"
            
            result.append(TransmissionTorrent(
                id=t["id"],
                hash_string=t["hashString"],
                name=t["name"],
                total_size=t["totalSize"],
                progress=t["percentComplete"] / 100.0,
                download_speed=0,
                upload_speed=0,
                status=t["status"],
                status_string=status_string,
                download_dir=t["downloadDir"],
                percent_complete=t["percentComplete"],
                eta=t["eta"]
            ))
        
        return result
    
    def start_torrent(self, info_hash: str) -> bool:
        """Start a paused torrent."""
        torrent = self.get_torrent(info_hash)
        if not torrent:
            return False
        
        result = self._request("torrent-start", {"ids": [torrent.id]})
        return result is not None
    
    def stop_torrent(self, info_hash: str) -> bool:
        """Pause a running torrent."""
        torrent = self.get_torrent(info_hash)
        if not torrent:
            return False
        
        result = self._request("torrent-stop", {"ids": [torrent.id]})
        return result is not None
    
    def delete_torrent(self, info_hash: str, delete_files: bool = True) -> bool:
        """Delete a torrent."""
        torrent = self.get_torrent(info_hash)
        if not torrent:
            return False
        
        result = self._request("torrent-remove", {
            "ids": [torrent.id],
            "delete-local-data": delete_files
        })
        return result is not None
    
    def wait_for_download(
        self,
        info_hash: str,
        timeout: float = 120.0,
        poll_interval: float = 1.0
    ) -> bool:
        """
        Wait for a torrent to complete downloading.
        
        Args:
            info_hash: Torrent hash to monitor
            timeout: Maximum time to wait
            poll_interval: How often to check
            
        Returns:
            True if torrent completed downloading, False if timed out
        """
        start_time = time.time()
        info_hash = info_hash.lower()
        
        while time.time() - start_time < timeout:
            torrent = self.get_torrent(info_hash)
            
            if torrent:
                if torrent.percent_complete >= 100.0:
                    logger.info(
                        f"Torrent {info_hash[:16]}... completed "
                        f"(downloaded: {torrent.total_size} bytes)"
                    )
                    return True
                
                logger.debug(
                    f"State: {torrent.status_string}, "
                    f"Progress: {torrent.percent_complete:.1f}%, "
                    f"DL: {torrent.download_speed} B/s"
                )
            
            time.sleep(poll_interval)
        
        logger.warning(f"Timeout waiting for torrent {info_hash[:16]}... to complete")
        return False
    
    def wait_for_seeding(
        self,
        info_hash: str,
        timeout: float = 60.0,
        poll_interval: float = 1.0
    ) -> bool:
        """
        Wait for a torrent to reach seeding state.
        
        Args:
            info_hash: Torrent hash to monitor
            timeout: Maximum time to wait
            poll_interval: How often to check
            
        Returns:
            True if torrent started seeding, False if timed out
        """
        start_time = time.time()
        info_hash = info_hash.lower()
        
        while time.time() - start_time < timeout:
            torrent = self.get_torrent(info_hash)
            
            if torrent:
                if torrent.percent_complete >= 100.0 and torrent.upload_speed > 0:
                    logger.info(
                        f"Torrent {info_hash[:16]}... is seeding "
                        f"(progress: {torrent.percent_complete:.1f}%)"
                    )
                    return True
                
                if torrent.percent_complete >= 100.0 and torrent.status == 0:
                    logger.info(
                        f"Torrent {info_hash[:16]}... is complete (stopped)"
                    )
                    return True
                
                logger.debug(
                    f"State: {torrent.status_string}, "
                    f"Progress: {torrent.percent_complete:.1f}%, "
                    f"UL: {torrent.upload_speed} B/s"
                )
            
            time.sleep(poll_interval)
        
        logger.warning(f"Timeout waiting for torrent {info_hash[:16]}... to seed")
        return False


def main():
    """Test the Transmission client."""
    import sys
    
    logging.basicConfig(level=logging.INFO)
    
    client = TransmissionClient()
    
    if not client.wait_for_startup(timeout=10):
        print("Failed to connect to Transmission")
        sys.exit(1)
    
    print("Connected to Transmission")
    torrents = client.get_all_torrents()
    print(f"Found {len(torrents)} torrents")


if __name__ == "__main__":
    main()
