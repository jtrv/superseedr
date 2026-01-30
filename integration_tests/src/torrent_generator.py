#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Torrent Generation Utility

Provides functionality to generate test torrent files in v1, v2, and hybrid formats.
"""

import hashlib
import os
import subprocess
import tempfile
from dataclasses import dataclass
from enum import Enum, auto
from pathlib import Path
from typing import List, Optional, Tuple


class TorrentVersion(Enum):
    """BitTorrent protocol version for torrent generation."""
    V1 = auto()
    V2 = auto()
    HYBRID = auto()


@dataclass
class TorrentInfo:
    """Information about a generated torrent."""
    torrent_path: Path
    info_hash: str
    data_path: Path
    files: List[Tuple[str, int]]  # (relative_path, size)


class TorrentGenerator:
    """Generates test torrent files and data using torrenttools or imktorrent."""
    
    def __init__(self, output_dir: Path, piece_size: int = 262144):
        """
        Initialize the torrent generator.
        
        Args:
            output_dir: Directory where torrents and test data will be created
            piece_size: Piece size in bytes (default: 256KB)
        """
        self.output_dir = Path(output_dir)
        self.piece_size = piece_size
        self.output_dir.mkdir(parents=True, exist_ok=True)
        
        # Check for available torrent creation tools
        self.tool = self._detect_tool()
    
    def _detect_tool(self) -> str:
        """Detect which torrent creation tool is available."""
        tools = ["torrenttools", "mktorrent", "imktorrent", "transmission-create"]
        
        for tool in tools:
            try:
                result = subprocess.run(
                    [tool, "--version"],
                    capture_output=True,
                    text=True,
                    timeout=5
                )
                if result.returncode in [0, 1]:  # Some tools return 1 for --version
                    return tool
            except (subprocess.TimeoutExpired, FileNotFoundError):
                continue
        
        raise RuntimeError(
            "No torrent creation tool found. Please install one of: "
            "torrenttools, mktorrent, imktorrent, or transmission-create"
        )
    
    def generate_test_data(
        self,
        name: str,
        total_size: int,
        num_files: int = 1
    ) -> Path:
        """
        Generate test binary data files.
        
        Args:
            name: Base name for the test data
            total_size: Total size in bytes
            num_files: Number of files to split data into
            
        Returns:
            Path to the directory containing the test data
        """
        data_dir = self.output_dir / "test_data" / name
        data_dir.mkdir(parents=True, exist_ok=True)
        
        # Calculate file sizes
        base_size = total_size // num_files
        remainder = total_size % num_files
        
        for i in range(num_files):
            file_size = base_size + (1 if i < remainder else 0)
            if num_files == 1:
                file_path = data_dir / f"{name}.bin"
            else:
                file_path = data_dir / f"file_{i:03d}.bin"
            
            # Generate deterministic pseudo-random data based on seed
            self._write_test_file(file_path, file_size, seed=i)
        
        return data_dir
    
    def _write_test_file(self, path: Path, size: int, seed: int = 0):
        """Write deterministic test data to a file."""
        # Use a simple PRNG for deterministic data
        chunk_size = 8192
        written = 0
        
        with open(path, "wb") as f:
            while written < size:
                to_write = min(chunk_size, size - written)
                # Deterministic pattern based on position and seed
                data = bytes((seed + (written + i) * 7) % 256 for i in range(to_write))
                f.write(data)
                written += to_write
    
    def create_torrent(
        self,
        data_path: Path,
        name: str,
        version: TorrentVersion,
        tracker_url: str = "http://tracker:6969/announce",
    ) -> TorrentInfo:
        """
        Create a torrent file from test data.
        
        Args:
            data_path: Path to the data to include in the torrent
            name: Name of the torrent
            version: Torrent protocol version (V1, V2, or HYBRID)
            tracker_url: Tracker announce URL
            
        Returns:
            TorrentInfo with paths and metadata
        """
        torrent_path = self.output_dir / f"{name}.torrent"
        
        # Build command based on available tool
        cmd = self._build_create_command(
            data_path, torrent_path, name, version, tracker_url
        )
        
        # Execute command
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            cwd=self.output_dir
        )
        
        if result.returncode != 0:
            raise RuntimeError(
                f"Failed to create torrent: {result.stderr}"
            )
        
        # Extract info hash
        info_hash = self._extract_info_hash(torrent_path)
        
        # Get file list
        files = self._list_files(data_path)
        
        return TorrentInfo(
            torrent_path=torrent_path,
            info_hash=info_hash,
            data_path=data_path,
            files=files
        )
    
    def _build_create_command(
        self,
        data_path: Path,
        torrent_path: Path,
        name: str,
        version: TorrentVersion,
        tracker_url: str
    ) -> List[str]:
        """Build the command for creating a torrent based on available tool."""
        
        if self.tool == "torrenttools":
            cmd = [
                "torrenttools", "create",
                "-o", str(torrent_path),
                "-t", tracker_url,
                "-l", str(self.piece_size),
                "--name", name,
            ]
            
            if version == TorrentVersion.V2:
                cmd.extend(["--v2-only"])
            elif version == TorrentVersion.HYBRID:
                cmd.extend(["--hybrid"])
            # V1 is default
            
            cmd.append(str(data_path))
            
        elif self.tool == "mktorrent":
            cmd = [
                "mktorrent",
                "-o", str(torrent_path),
                "-a", tracker_url,
                "-l", str(self._calculate_piece_size_exponent()),
                "-n", name,
            ]
            
            if version == TorrentVersion.V2:
                # mktorrent may not support v2 only
                raise NotImplementedError(
                    "mktorrent may not support v2-only torrents. "
                    "Use torrenttools for v2 torrents."
                )
            elif version == TorrentVersion.HYBRID:
                if self._check_mktorrent_v2_support():
                    cmd.append("--hybrid")
            
            cmd.append(str(data_path))
            
        elif self.tool == "transmission-create":
            cmd = [
                "transmission-create",
                "-o", str(torrent_path),
                "-t", tracker_url,
                "-p",  # private flag
            ]
            
            if version != TorrentVersion.V1:
                raise NotImplementedError(
                    "transmission-create only supports v1 torrents. "
                    "Use torrenttools for v2/hybrid torrents."
                )
            
            cmd.append(str(data_path))
        
        else:
            raise RuntimeError(f"Unsupported tool: {self.tool}")
        
        return cmd
    
    def _calculate_piece_size_exponent(self) -> int:
        """Calculate the piece size exponent for mktorrent."""
        # mktorrent uses exponent where piece_size = 2^exponent
        import math
        return int(math.log2(self.piece_size))
    
    def _check_mktorrent_v2_support(self) -> bool:
        """Check if mktorrent supports v2/hybrid torrents."""
        try:
            result = subprocess.run(
                ["mktorrent", "--help"],
                capture_output=True,
                text=True,
                timeout=5
            )
            return "--hybrid" in result.stdout or "--v2" in result.stdout
        except:
            return False
    
    def _extract_info_hash(self, torrent_path: Path) -> str:
        """Extract the info hash from a torrent file."""
        # Use torrenttools or python to parse
        try:
            result = subprocess.run(
                ["torrenttools", "info", str(torrent_path), "-f", "json"],
                capture_output=True,
                text=True,
                timeout=10
            )
            
            if result.returncode == 0:
                import json
                info = json.loads(result.stdout)
                return info.get("info_hash", "")
        except:
            pass
        
        # Fallback: parse with Python
        return self._parse_torrent_info_hash(torrent_path)
    
    def _parse_torrent_info_hash(self, torrent_path: Path) -> str:
        """Parse torrent file to extract info hash using Python."""
        try:
            import bencodepy
            
            with open(torrent_path, "rb") as f:
                data = bencodepy.decode(f.read())
            
            # Get the info dictionary
            info = data.get(b"info", {})
            
            # Re-encode just the info dict and hash it
            info_bencoded = bencodepy.encode(info)
            info_hash = hashlib.sha1(info_bencoded).hexdigest()
            
            return info_hash
        except ImportError:
            # If bencodepy not available, return placeholder
            return "unknown"
    
    def _list_files(self, data_path: Path) -> List[Tuple[str, int]]:
        """List all files in the data directory with their sizes."""
        files = []
        
        if data_path.is_file():
            files.append((data_path.name, data_path.stat().st_size))
        else:
            for file_path in data_path.rglob("*"):
                if file_path.is_file():
                    rel_path = file_path.relative_to(data_path)
                    files.append((str(rel_path), file_path.stat().st_size))
        
        return sorted(files)
    
    def create_test_torrent(
        self,
        name: str,
        version: TorrentVersion,
        size: int = 1048576,  # 1MB default
        num_files: int = 1,
        tracker_url: str = "http://tracker:6969/announce",
    ) -> TorrentInfo:
        """
        Convenience method to generate test data and create a torrent.
        
        Args:
            name: Name for the test
            version: Torrent protocol version
            size: Total size of test data in bytes
            num_files: Number of files to create
            tracker_url: Tracker announce URL
            
        Returns:
            TorrentInfo with all metadata
        """
        data_path = self.generate_test_data(name, size, num_files)
        return self.create_torrent(data_path, name, version, tracker_url)
    
    def calculate_sha1(self, file_path: Path) -> str:
        """Calculate SHA1 hash of a file for verification."""
        sha1 = hashlib.sha1()
        
        with open(file_path, "rb") as f:
            while chunk := f.read(8192):
                sha1.update(chunk)
        
        return sha1.hexdigest()


def main():
    """CLI for testing the torrent generator."""
    import argparse
    
    parser = argparse.ArgumentParser(description="Generate test torrents")
    parser.add_argument("output_dir", help="Output directory for torrents")
    parser.add_argument("--name", default="test", help="Torrent name")
    parser.add_argument(
        "--version",
        choices=["v1", "v2", "hybrid"],
        default="v1",
        help="Torrent version"
    )
    parser.add_argument(
        "--size",
        type=int,
        default=1048576,
        help="Total data size in bytes"
    )
    parser.add_argument(
        "--files",
        type=int,
        default=1,
        help="Number of files"
    )
    
    args = parser.parse_args()
    
    version_map = {
        "v1": TorrentVersion.V1,
        "v2": TorrentVersion.V2,
        "hybrid": TorrentVersion.HYBRID
    }
    
    generator = TorrentGenerator(args.output_dir)
    info = generator.create_test_torrent(
        name=args.name,
        version=version_map[args.version],
        size=args.size,
        num_files=args.files
    )
    
    print(f"Created torrent: {info.torrent_path}")
    print(f"Info hash: {info.info_hash}")
    print(f"Data path: {info.data_path}")
    print(f"Files: {info.files}")


if __name__ == "__main__":
    main()
