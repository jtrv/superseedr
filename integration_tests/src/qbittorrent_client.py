#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
qBittorrent API Client

Provides a wrapper around the qBittorrent Web API for controlling
the reference client during integration tests.
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
class QBittorrentTorrent:
    """Information about a torrent in qBittorrent."""
    hash: str
    name: str
    size: int
    progress: float
    dlspeed: int
    upspeed: int
    priority: int
    num_seeds: int
    num_leechs: int
    ratio: float
    eta: int
    state: str
    seq_dl: bool
    f_l_piece_prio: bool
    c_category: Optional[str]
    tags: List[str]
    save_path: str
    downloaded: int
    uploaded: int


class QBittorrentClient:
    """Client for qBittorrent Web API."""
    
    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        username: str = "admin",
        password: str = "adminadmin",
        timeout: int = 30
    ):
        """
        Initialize qBittorrent client.
        
        Args:
            base_url: Base URL for qBittorrent WebUI
            username: WebUI username
            password: WebUI password
            timeout: Request timeout in seconds
        """
        self.base_url = base_url.rstrip("/")
        self.username = username
        self.password = password
        self.timeout = timeout
        self._session = requests.Session()
        self._authenticated = False
        
        # Setup retry strategy
        retry_strategy = Retry(
            total=3,
            backoff_factor=1,
            status_forcelist=[429, 500, 502, 503, 504],
        )
        adapter = HTTPAdapter(max_retries=retry_strategy)
        self._session.mount("http://", adapter)
        self._session.mount("https://", adapter)
    
    def authenticate(self) -> bool:
        """
        Authenticate with qBittorrent.
        
        Returns:
            True if authentication successful
        """
        try:
            response = self._session.post(
                f"{self.base_url}/api/v2/auth/login",
                data={
                    "username": self.username,
                    "password": self.password
                },
                timeout=self.timeout
            )
            
            if response.status_code == 200 and response.text == "Ok.":
                self._authenticated = True
                logger.info("Authenticated with qBittorrent")
                return True
            else:
                logger.error(f"Authentication failed: {response.status_code} - {response.text}")
                return False
                
        except Exception as e:
            logger.error(f"Authentication error: {e}")
            return False
    
    def _ensure_auth(self):
        """Ensure client is authenticated."""
        if not self._authenticated:
            if not self.authenticate():
                raise RuntimeError("Failed to authenticate with qBittorrent")
    
    def _request(self, method: str, endpoint: str, **kwargs) -> requests.Response:
        """Make an authenticated request."""
        self._ensure_auth()
        
        url = f"{self.base_url}{endpoint}"
        response = self._session.request(
            method, url, timeout=self.timeout, **kwargs
        )
        
        # Handle session expiration
        if response.status_code == 403:
            logger.warning("Session expired, re-authenticating...")
            self._authenticated = False
            self.authenticate()
            response = self._session.request(
                method, url, timeout=self.timeout, **kwargs
            )
        
        response.raise_for_status()
        return response
    
    def add_torrent(
        self,
        torrent_path: Optional[Path] = None,
        magnet_link: Optional[str] = None,
        save_path: Optional[str] = None,
        skip_checking: bool = False,
        paused: bool = False,
        category: Optional[str] = None,
        tags: Optional[List[str]] = None,
        upload_limit: Optional[int] = None,
        download_limit: Optional[int] = None,
        ratio_limit: Optional[float] = None,
        seeding_time_limit: Optional[int] = None,
    ) -> bool:
        """
        Add a torrent to qBittorrent.
        
        Args:
            torrent_path: Path to .torrent file
            magnet_link: Magnet URI
            save_path: Download location
            skip_checking: Skip hash checking (for seed mode)
            paused: Add in paused state
            category: Torrent category
            tags: List of tags
            upload_limit: Upload rate limit (bytes/s)
            download_limit: Download rate limit (bytes/s)
            ratio_limit: Share ratio limit
            seeding_time_limit: Seeding time limit (minutes)
            
        Returns:
            True if successful
        """
        data = {
            "skip_checking": "true" if skip_checking else "false",
            "paused": "true" if paused else "false",
        }
        
        if save_path:
            data["savepath"] = save_path
        if category:
            data["category"] = category
        if tags:
            data["tags"] = ",".join(tags)
        if upload_limit is not None:
            data["upLimit"] = upload_limit
        if download_limit is not None:
            data["dlLimit"] = download_limit
        if ratio_limit is not None:
            data["ratioLimit"] = ratio_limit
        if seeding_time_limit is not None:
            data["seedingTimeLimit"] = seeding_time_limit
        
        files = {}
        if torrent_path:
            files["torrents"] = open(torrent_path, "rb")
        elif magnet_link:
            data["urls"] = magnet_link
        else:
            raise ValueError("Either torrent_path or magnet_link must be provided")
        
        try:
            response = self._request(
                "POST",
                "/api/v2/torrents/add",
                data=data,
                files=files if files else None
            )
            
            if response.status_code in [200, 204]:
                logger.info(f"Added torrent: {torrent_path or magnet_link[:50]}...")
                return True
            else:
                logger.error(f"Failed to add torrent: {response.status_code}")
                return False
                
        finally:
            if files:
                for f in files.values():
                    f.close()
    
    def get_torrents(self, category: Optional[str] = None) -> List[QBittorrentTorrent]:
        """
        Get list of all torrents.
        
        Args:
            category: Filter by category
            
        Returns:
            List of QBittorrentTorrent objects
        """
        params = {}
        if category:
            params["category"] = category
        
        response = self._request(
            "GET",
            "/api/v2/torrents/info",
            params=params
        )
        
        data = response.json()
        torrents = []
        
        for item in data:
            torrents.append(QBittorrentTorrent(
                hash=item.get("hash", ""),
                name=item.get("name", ""),
                size=item.get("size", 0),
                progress=item.get("progress", 0.0),
                dlspeed=item.get("dlspeed", 0),
                upspeed=item.get("upspeed", 0),
                priority=item.get("priority", 0),
                num_seeds=item.get("num_seeds", 0),
                num_leechs=item.get("num_leechs", 0),
                ratio=item.get("ratio", 0.0),
                eta=item.get("eta", 0),
                state=item.get("state", ""),
                seq_dl=item.get("seq_dl", False),
                f_l_piece_prio=item.get("f_l_piece_prio", False),
                c_category=item.get("category"),
                tags=item.get("tags", "").split(",") if item.get("tags") else [],
                save_path=item.get("save_path", ""),
                downloaded=item.get("downloaded", 0),
                uploaded=item.get("uploaded", 0),
            ))
        
        return torrents
    
    def get_torrent(self, info_hash: str) -> Optional[QBittorrentTorrent]:
        """Get a specific torrent by info hash."""
        torrents = self.get_torrents()
        for torrent in torrents:
            if torrent.hash.lower() == info_hash.lower():
                return torrent
        return None
    
    def delete_torrent(self, info_hash: str, delete_files: bool = True) -> bool:
        """
        Delete a torrent.
        
        Args:
            info_hash: Torrent hash to delete
            delete_files: Also delete downloaded files
            
        Returns:
            True if successful
        """
        response = self._request(
            "POST",
            "/api/v2/torrents/delete",
            data={
                "hashes": info_hash,
                "deleteFiles": "true" if delete_files else "false"
            }
        )
        
        return response.status_code in [200, 204]
    
    def recheck_torrent(self, info_hash: str) -> bool:
        """Force recheck of torrent."""
        response = self._request(
            "POST",
            "/api/v2/torrents/recheck",
            data={"hashes": info_hash}
        )
        return response.status_code in [200, 204]
    
    def pause_torrent(self, info_hash: str) -> bool:
        """Pause a torrent."""
        response = self._request(
            "POST",
            "/api/v2/torrents/pause",
            data={"hashes": info_hash}
        )
        return response.status_code in [200, 204]
    
    def resume_torrent(self, info_hash: str) -> bool:
        """Resume a torrent."""
        response = self._request(
            "POST",
            "/api/v2/torrents/resume",
            data={"hashes": info_hash}
        )
        return response.status_code in [200, 204]
    
    def get_upload_rate(self, info_hash: str) -> int:
        """
        Get current upload rate for a torrent.
        
        Returns:
            Upload rate in bytes/second
        """
        torrent = self.get_torrent(info_hash)
        if torrent:
            return torrent.upspeed
        return 0
    
    def get_download_rate(self, info_hash: str) -> int:
        """Get current download rate for a torrent."""
        torrent = self.get_torrent(info_hash)
        if torrent:
            return torrent.dlspeed
        return 0
    
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
        
        while time.time() - start_time < timeout:
            torrent = self.get_torrent(info_hash)
            
            if torrent:
                if torrent.state in ["uploading", "stalledUP", "forcedUP", "allocating"]:
                    logger.info(
                        f"Torrent {info_hash[:16]}... is seeding "
                        f"(progress: {torrent.progress*100:.1f}%)"
                    )
                    return True
                
                logger.debug(
                    f"State: {torrent.state}, Progress: {torrent.progress*100:.1f}%, "
                    f"DL: {torrent.dlspeed} B/s, UL: {torrent.upspeed} B/s"
                )
            
            time.sleep(poll_interval)
        
        logger.warning(f"Timeout waiting for torrent {info_hash[:16]}... to seed")
        return False
    
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
            True if download completed, False if timed out
        """
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            torrent = self.get_torrent(info_hash)
            
            if torrent:
                if torrent.progress >= 1.0 and torrent.state in ["uploading", "stalledUP"]:
                    logger.info(
                        f"Torrent {info_hash[:16]}... download complete "
                        f"(DL: {torrent.downloaded}, UL: {torrent.uploaded})"
                    )
                    return True
                
                logger.debug(
                    f"Progress: {torrent.progress*100:.1f}%, "
                    f"DL: {torrent.dlspeed} B/s, State: {torrent.state}"
                )
            
            time.sleep(poll_interval)
        
        logger.warning(f"Timeout waiting for torrent {info_hash[:16]}... to download")
        return False
    
    def is_alive(self) -> bool:
        """Check if qBittorrent is running and accessible."""
        try:
            response = self._session.get(
                f"{self.base_url}/api/v2/app/version",
                timeout=5
            )
            return response.status_code == 200
        except:
            return False
    
    def wait_for_startup(self, timeout: float = 60.0, poll_interval: float = 2.0) -> bool:
        """
        Wait for qBittorrent to become available.
        
        Args:
            timeout: Maximum time to wait
            poll_interval: How often to check
            
        Returns:
            True if qBittorrent is ready, False if timed out
        """
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            if self.is_alive():
                logger.info("qBittorrent is ready")
                return True
            time.sleep(poll_interval)
        
        logger.error(f"Timeout waiting for qBittorrent to start")
        return False


def main():
    """CLI for testing the qBittorrent client."""
    import argparse
    
    parser = argparse.ArgumentParser(description="qBittorrent API Client")
    parser.add_argument("--url", default="http://localhost:8080", help="qBittorrent URL")
    parser.add_argument("--user", default="admin", help="Username")
    parser.add_argument("--pass", default="adminadmin", dest="password", help="Password")
    
    subparsers = parser.add_subparsers(dest="command", help="Command")
    
    # List command
    subparsers.add_parser("list", help="List all torrents")
    
    # Add command
    add_parser = subparsers.add_parser("add", help="Add a torrent")
    add_parser.add_argument("torrent", help="Path to torrent file")
    add_parser.add_argument("--skip-check", action="store_true", help="Skip hash check")
    
    # Delete command
    del_parser = subparsers.add_parser("delete", help="Delete a torrent")
    del_parser.add_argument("hash", help="Torrent hash")
    
    args = parser.parse_args()
    
    client = QBittorrentClient(args.url, args.user, args.password)
    
    if not client.authenticate():
        print("Authentication failed")
        return 1
    
    if args.command == "list":
        torrents = client.get_torrents()
        for t in torrents:
            print(f"{t.hash[:16]}... - {t.name[:40]} - {t.state} - {t.progress*100:.1f}%")
    
    elif args.command == "add":
        if client.add_torrent(torrent_path=Path(args.torrent), skip_checking=args.skip_check):
            print("Torrent added successfully")
        else:
            print("Failed to add torrent")
    
    elif args.command == "delete":
        if client.delete_torrent(args.hash):
            print("Torrent deleted")
        else:
            print("Failed to delete torrent")
    
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
