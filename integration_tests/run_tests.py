#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 The superseedr Contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Superseedr Integration Test Runner

This script orchestrates the integration testing environment for Superseedr
against reference BitTorrent clients (qBittorrent/Transmission).
"""

import argparse
import json
import logging
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional

# Add src directory to path for importing test modules
sys.path.insert(0, str(Path(__file__).parent / "src"))

from superseedr_monitor import SuperseedrMonitor
from qbittorrent_client import QBittorrentClient
from torrent_generator import TorrentGenerator, TorrentVersion

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s",
    datefmt="%H:%M:%S"
)
logger = logging.getLogger(__name__)


class IntegrationTestRunner:
    """Main test runner that orchestrates the integration test environment."""
    
    def __init__(self, compose_file: str = "docker-compose.yml"):
        self.compose_file = compose_file
        self.project_dir = Path(__file__).parent
        self.data_dir = self.project_dir / "data"
        
        # Data directories
        self.tracker_data = self.data_dir / "tracker_data"
        self.superseedr_watch = self.data_dir / "superseedr_watch"
        self.superseedr_downloads = self.data_dir / "superseedr_downloads"
        self.superseedr_status = self.data_dir / "superseedr_status"
        self.reference_downloads = self.data_dir / "reference_client_downloads"
        self.qbittorrent_config = self.data_dir / "qbittorrent_config"
        
        # Test clients
        self.superseedr = SuperseedrMonitor(self.superseedr_status)
        self.qbittorrent = QBittorrentClient("http://localhost:8080")
    
    def setup(self):
        """Clean data directories and prepare the environment."""
        logger.info("Setting up test environment...")
        
        # Clean all data directories
        dirs_to_clean = [
            self.tracker_data,
            self.superseedr_watch,
            self.superseedr_downloads,
            self.superseedr_status,
            self.reference_downloads,
            self.qbittorrent_config,
        ]
        
        for dir_path in dirs_to_clean:
            if dir_path.exists():
                logger.info(f"Cleaning {dir_path}")
                shutil.rmtree(dir_path)
            dir_path.mkdir(parents=True, exist_ok=True)
        
        # Ensure subdirectories exist
        (self.superseedr_status / "status_files").mkdir(exist_ok=True)
        
        logger.info("Test environment setup complete")
    
    def teardown(self):
        """Clean up and stop containers."""
        logger.info("Tearing down test environment...")
        
        # Stop containers
        self._run_compose(["down", "-v"], check=False)
        
        # Clean data directories
        dirs_to_clean = [
            self.tracker_data,
            self.superseedr_watch,
            self.superseedr_downloads,
            self.superseedr_status,
            self.reference_downloads,
            self.qbittorrent_config,
        ]
        
        for dir_path in dirs_to_clean:
            if dir_path.exists():
                logger.info(f"Removing {dir_path}")
                shutil.rmtree(dir_path)
        
        logger.info("Teardown complete")
    
    def start_services(self):
        """Start Docker Compose services."""
        logger.info("Starting Docker Compose services...")
        
        # Build and start services
        self._run_compose(["up", "-d", "--build"])
        
        # Wait for services to be healthy
        logger.info("Waiting for services to be healthy...")
        self._wait_for_services()
        
        logger.info("All services are healthy and ready")
    
    def stop_services(self):
        """Stop Docker Compose services."""
        logger.info("Stopping Docker Compose services...")
        self._run_compose(["down"])
    
    def _run_compose(self, args: list, check: bool = True):
        """Run docker-compose command."""
        cmd = ["docker", "compose", "-f", self.compose_file] + args
        logger.debug(f"Running: {' '.join(cmd)}")
        
        result = subprocess.run(
            cmd,
            cwd=self.project_dir,
            capture_output=True,
            text=True
        )
        
        if result.returncode != 0 and check:
            logger.error(f"Docker compose command failed: {result.stderr}")
            raise RuntimeError(f"Docker compose failed: {result.stderr}")
        
        return result
    
    def _wait_for_services(self, timeout: int = 120):
        """Wait for all services to be healthy."""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                result = self._run_compose(["ps", "--format", "json"], check=False)
                if result.returncode == 0:
                    services = json.loads(result.stdout)
                    all_healthy = True
                    
                    for service in services:
                        health = service.get("Health", "")
                        state = service.get("State", "")
                        
                        if state != "running" or health != "healthy":
                            all_healthy = False
                            break
                    
                    if all_healthy:
                        return True
                
                time.sleep(2)
            except Exception as e:
                logger.warning(f"Error checking service health: {e}")
                time.sleep(2)
        
        raise TimeoutError(f"Services did not become healthy within {timeout} seconds")
    
    def get_container_logs(self, service: str) -> str:
        """Get logs from a specific service."""
        result = self._run_compose(["logs", service], check=False)
        return result.stdout
    
    def save_artifacts(self, test_name: str):
        """Save container logs and status files for debugging."""
        artifact_dir = self.project_dir / "artifacts" / test_name
        artifact_dir.mkdir(parents=True, exist_ok=True)
        
        # Save logs
        for service in ["tracker", "superseedr", "qbittorrent"]:
            logs = self.get_container_logs(service)
            (artifact_dir / f"{service}_logs.txt").write_text(logs)
        
        # Save status files
        if self.superseedr_status.exists():
            status_artifact_dir = artifact_dir / "status_files"
            status_artifact_dir.mkdir(exist_ok=True)
            
            for status_file in self.superseedr_status.rglob("*.json"):
                rel_path = status_file.relative_to(self.superseedr_status)
                dest = status_artifact_dir / rel_path
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(status_file, dest)
        
        logger.info(f"Artifacts saved to {artifact_dir}")


def main():
    parser = argparse.ArgumentParser(description="Superseedr Integration Test Runner")
    parser.add_argument(
        "--scenario",
        choices=["all", "v1", "v2", "hybrid", "seeding"],
        default="all",
        help="Which test scenario to run"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Enable verbose logging"
    )
    parser.add_argument(
        "--keep",
        action="store_true",
        help="Keep containers running after tests"
    )
    
    args = parser.parse_args()
    
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    
    runner = IntegrationTestRunner()
    
    try:
        # Setup
        runner.setup()
        runner.start_services()
        
        # Run tests based on scenario
        from scenarios import run_test_scenario
        
        if args.scenario == "all":
            scenarios = ["v1", "v2", "hybrid", "seeding"]
        else:
            scenarios = [args.scenario]
        
        results = {}
        for scenario in scenarios:
            logger.info(f"\n{'='*60}")
            logger.info(f"Running Scenario: {scenario.upper()}")
            logger.info(f"{'='*60}\n")
            
            try:
                success = run_test_scenario(runner, scenario)
                results[scenario] = "PASSED" if success else "FAILED"
            except Exception as e:
                logger.error(f"Scenario {scenario} failed with exception: {e}")
                results[scenario] = "ERROR"
                runner.save_artifacts(f"{scenario}_failure")
                raise
        
        # Print results
        logger.info(f"\n{'='*60}")
        logger.info("TEST RESULTS")
        logger.info(f"{'='*60}")
        for scenario, result in results.items():
            status = "✓" if result == "PASSED" else "✗"
            logger.info(f"{status} {scenario}: {result}")
        
        all_passed = all(r == "PASSED" for r in results.values())
        sys.exit(0 if all_passed else 1)
        
    except KeyboardInterrupt:
        logger.info("Tests interrupted by user")
        sys.exit(130)
    except Exception as e:
        logger.error(f"Test execution failed: {e}")
        sys.exit(1)
    finally:
        if not args.keep:
            runner.teardown()
        else:
            logger.info("Containers kept running. Use 'docker compose down' to stop them.")


if __name__ == "__main__":
    main()
