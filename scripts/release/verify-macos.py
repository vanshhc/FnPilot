#!/usr/bin/env python3
import argparse
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description="Verify an official Developer ID app; never accepts Apple Development or ad-hoc signing.")
parser.add_argument("app", type=Path)
parser.add_argument("--team-id", required=True)
args = parser.parse_args()
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(args.app)], check=True)
identity = subprocess.run(["codesign", "-dv", "--verbose=4", str(args.app)], capture_output=True, text=True, check=True).stderr
if f"TeamIdentifier={args.team_id}" not in identity or "Authority=Developer ID Application:" not in identity or "runtime" not in identity:
    raise SystemExit("Official release identity or hardened runtime is missing.")
subprocess.run(["spctl", "--assess", "--type", "execute", "--verbose=2", str(args.app)], check=True)
subprocess.run(["xcrun", "stapler", "validate", str(args.app)], check=True)
print("Developer ID, Gatekeeper and stapled notarization verified.")
