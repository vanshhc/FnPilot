#!/usr/bin/env python3
"""Sign local builds with the pinned Apple identity; never fall back to ad hoc."""

import argparse
import json
from pathlib import Path
import plistlib
import re
import subprocess
import sys


def run(*args: str) -> str:
    result = subprocess.run(args, capture_output=True, text=True, check=True)
    return result.stdout + result.stderr


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", nargs="?", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--config", type=Path, default=Path(__file__).resolve().parent.parent / "signing.json")
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    identity = config["identity_sha1"].upper()
    if not re.fullmatch(r"[0-9A-F]{40}", identity):
        raise ValueError("signing.json must contain an exact certificate SHA-1 identity.")
    identities = run("security", "find-identity", "-v", "-p", "codesigning")
    available = set(re.findall(r'^\s*\d+\)\s+([0-9A-F]{40})\s+"', identities, re.MULTILINE))
    if identity not in available:
        raise ValueError("The pinned signing certificate/private key is unavailable or expired. Restore it in Keychain or deliberately migrate signing.json. No ad-hoc fallback is allowed.")
    if args.check:
        print("Pinned signing identity is available.")
        return
    if args.app is None:
        parser.error("app is required unless --check is used")
    app = args.app.resolve()
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    if info.get("CFBundleIdentifier") != config["bundle_id"]:
        raise ValueError("App bundle identifier does not match signing.json.")

    nested = [app / "Contents/MacOS/fnpilot-cli"]
    runtime = app / "Contents/Resources/Runtime"
    if runtime.is_dir():
        nested += list(runtime.glob("*.dylib"))
        nested.append(runtime / "llama-server")
    # Runtime symlinks must be signed once, through their actual files.
    for binary in sorted({path.resolve() for path in nested if path.is_file()}):
        if not binary.is_relative_to(app):
            raise ValueError("Nested code points outside the app bundle.")
        run("codesign", "--force", "--sign", identity, "--timestamp=none", str(binary))
    run("codesign", "--force", "--sign", identity, "--timestamp=none", str(app))
    details = run("codesign", "-dv", "--verbose=2", str(app))
    if f"TeamIdentifier={config['team_id']}\n" not in details:
        raise ValueError("Signed app has an unexpected Apple team. Do not install this build.")
    requirement = run("codesign", "-d", "-r-", str(app))
    if "anchor apple" not in requirement or "cdhash H" in requirement:
        raise ValueError("App does not have a stable Apple-backed designated requirement.")
    run("codesign", "--verify", "--deep", "--strict", str(app))
    print("Signed and verified with the pinned Apple identity.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Signing failed: {error}", file=sys.stderr)
        sys.exit(1)
