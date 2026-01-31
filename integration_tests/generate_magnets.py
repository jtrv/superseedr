#!/usr/bin/env python3
"""
Generate magnet links from .torrent files.

Usage:
    python3 generate_magnets.py              # Generate for all torrents
    python3 generate_magnets.py seeding_v2   # Generate for specific torrent
"""

import hashlib
import sys
from pathlib import Path
from urllib.parse import quote

import bencodepy

TORRENTS_DIR = Path(__file__).parent / "torrents"
MAGNETS_FILE = TORRENTS_DIR / "magnets.txt"


def generate_magnet(torrent_path: Path) -> str:
    """Generate a magnet link from a .torrent file."""
    with open(torrent_path, "rb") as f:
        data = bencodepy.decode(f.read())

    info = data.get(b"info", {})
    info_bencoded = bencodepy.encode(info)
    info_hash = hashlib.sha1(info_bencoded).hexdigest()

    name = info.get(b"name", b"unknown").decode()
    display_name = quote(name, safe="")

    magnet = f"magnet:?xt=urn:btih:{info_hash}&dn={display_name}"

    if b"announce" in data:
        tracker = data[b"announce"].decode()
        magnet += f"&tr={quote(tracker)}"

    if b"announce-list" in data:
        for tracker in data[b"announce-list"]:
            if isinstance(tracker, bytes):
                tracker_url = tracker.decode()
            else:
                tracker_url = str(tracker)
            magnet += f"&tr={quote(tracker_url)}"

    return magnet


def main():
    """Main entry point."""
    if len(sys.argv) > 1:
        # Generate for specific torrent
        torrent_name = sys.argv[1]
        if not torrent_name.endswith(".torrent"):
            torrent_name += ".torrent"
        torrent_path = TORRENTS_DIR / torrent_name
        if torrent_path.exists():
            magnet = generate_magnet(torrent_path)
            print(f"{torrent_path.name}:")
            print(f"  {magnet}")
        else:
            print(f"Error: Torrent not found: {torrent_path}")
            sys.exit(1)
    else:
        # Generate for all torrents
        magnets = []
        print("Generating magnet links for all torrents...\n")

        for torrent_path in sorted(TORRENTS_DIR.glob("*.torrent")):
            magnet = generate_magnet(torrent_path)
            magnets.append((torrent_path.name, magnet))
            print(f"{torrent_path.name}:")
            print(f"  {magnet}\n")

        # Save to magnets.txt
        with open(MAGNETS_FILE, "w") as f:
            for name, magnet in magnets:
                f.write(f"# {name}\n")
                f.write(f"{magnet}\n\n")

        print(f"Magnet links saved to: {MAGNETS_FILE}")


if __name__ == "__main__":
    main()
