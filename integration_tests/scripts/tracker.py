#!/usr/bin/env python3
"""Simple BitTorrent tracker."""

from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import parse_qs

TRACKERS = {}

def bencode(data):
    """Simple bencode encoder."""
    if isinstance(data, int):
        return f"i{data}e".encode()
    elif isinstance(data, str):
        s = data.encode("utf-8")
        return f"{len(s)}:{s.decode()}".encode()
    elif isinstance(data, bytes):
        return f"{len(data)}:{data.decode('latin-1')}".encode()
    elif isinstance(data, list):
        result = b"l"
        for item in data:
            result += bencode(item)
        return result + b"e"
    elif isinstance(data, dict):
        result = b"d"
        for key in sorted(data.keys()):
            result += bencode(key) + bencode(data[key])
        return result + b"e"
    return b""

class TrackerHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        pass
    
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        
        if parsed.path != "/announce":
            self.send_error(404, "Not Found")
            return
        
        query = parse_qs(parsed.query)
        
        info_hash = query.get("info_hash", [b""])[0]
        if isinstance(info_hash, bytes):
            info_hash = info_hash.hex()
        
        peer_id = query.get("peer_id", [b""])[0]
        if isinstance(peer_id, bytes):
            peer_id = peer_id.decode("latin-1", errors="replace")
        
        port = int(query.get("port", ["0"])[0])
        event = query.get("event", [""])[0]
        uploaded = int(query.get("uploaded", ["0"])[0])
        downloaded = int(query.get("downloaded", ["0"])[0])
        left = int(query.get("left", ["0"])[0])
        
        if info_hash not in TRACKERS:
            TRACKERS[info_hash] = {}
        
        TRACKERS[info_hash][peer_id] = {
            "ip": self.client_address[0],
            "port": port,
            "uploaded": uploaded,
            "downloaded": downloaded,
            "left": left,
            "event": event,
        }
        
        peers = []
        for pid, peer in TRACKERS[info_hash].items():
            if pid != peer_id and peer["left"] == 0 and peer["port"] > 0:
                peers.append({
                    "peer id": pid,
                    "ip": peer["ip"],
                    "port": peer["port"],
                })
        
        response = {
            "interval": 60,
            "complete": sum(1 for p in TRACKERS[info_hash].values() if p["left"] == 0),
            "incomplete": sum(1 for p in TRACKERS[info_hash].values() if p["left"] > 0),
            "peers": peers,
        }
        
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.end_headers()
        self.wfile.write(bencode(response))

if __name__ == "__main__":
    import urllib.parse
    server = HTTPServer(("0.0.0.0", 6969), TrackerHandler)
    print("Tracker running on port 6969...")
    server.serve_forever()
