# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Integration test scenarios for Superseedr.
"""

from .qbittorrent_client import QBittorrentClient
from .transmission_client import TransmissionClient

__all__ = [
    "QBittorrentClient",
    "TransmissionClient",
]
