#!/usr/bin/env python3
"""Collect the resources actually signed in the native candidate, then inventory them."""
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
destination = root / "target/distribution"
destination.mkdir(parents=True, exist_ok=True)
resources = root / "apps/desktop/src-tauri/resources"
if sys.platform == "darwin":
    resources = root / "target/release/bundle/macos/FnPilot.app/Contents/Resources"
for name in ["cli", "runtime", "licenses"]:
    shutil.copytree(resources / name, destination / name, dirs_exist_ok=True)
shutil.copy2(resources / "dependencies.json", destination / "dependencies.json")
subprocess.run([sys.executable, "scripts/release/inventory.py", str(destination), str(root / "target/release-manifest.json"), "--official"], cwd=root, check=True)
