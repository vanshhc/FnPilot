#!/usr/bin/env python3
"""Check update identity continuity in scratch copies without launching the app."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile


def command(*args: str, succeeds: bool = True) -> str:
    result = subprocess.run(args, capture_output=True, text=True)
    if (result.returncode == 0) != succeeds:
        raise RuntimeError(f"Unexpected exit code {result.returncode}: {args[0]}: {result.stderr.strip()}")
    return result.stdout + result.stderr


def requirement(app: Path) -> str:
    output = command("codesign", "-d", "-r-", str(app))
    return next(line.split("designated => ", 1)[1] for line in output.splitlines() if "designated => " in line)


def cdhash(app: Path) -> str:
    output = command("codesign", "-dv", "--verbose=4", str(app))
    return re.search(r"^CDHash=(.+)$", output, re.MULTILINE)[1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("report", type=Path)
    parser.add_argument("--work-dir", type=Path, required=True)
    args = parser.parse_args()
    project = Path(__file__).resolve().parent.parent
    signer = str(project / "scripts/sign-app.py")
    config = json.loads((project / "signing.json").read_text())
    command("codesign", "--verify", "--deep", "--strict", str(args.app))
    first_requirement = requirement(args.app)
    if "anchor apple" not in first_requirement or "cdhash H" in first_requirement:
        raise RuntimeError("Baseline app does not have a stable Apple identity.")
    with tempfile.TemporaryDirectory(prefix="fnpilot-signing-", dir=args.work_dir) as scratch:
        scratch = Path(scratch)
        updated = scratch / "FnPilot.app"
        shutil.copytree(args.app, updated, symlinks=True)
        info_file = updated / "Contents/Info.plist"
        info = plistlib.loads(info_file.read_bytes())
        info["CFBundleVersion"] = str(int(info["CFBundleVersion"]) + 1)
        info_file.write_bytes(plistlib.dumps(info))
        command(sys.executable, signer, str(updated))
        updated_requirement = requirement(updated)
        if first_requirement != updated_requirement or cdhash(args.app) == cdhash(updated):
            raise RuntimeError("Update must have a different code hash and identical designated requirement.")
        command("codesign", "--verify", "--strict", "-R=" + first_requirement, str(updated))
        command("codesign", "--verify", "--strict", "-R=" + updated_requirement, str(args.app))
        hashes = {"first": cdhash(args.app), "update": cdhash(updated)}
        # An ad-hoc signature must not inherit approval for the Apple-signed app.
        command("codesign", "--force", "--sign", "-", str(updated))
        command("codesign", "--verify", "--strict", "-R=" + first_requirement, str(updated), succeeds=False)
        missing_config = scratch / "missing-identity.json"
        missing_config.write_text(json.dumps({**config, "identity_sha1": "0" * 40}))
        command(sys.executable, signer, "--config", str(missing_config), "--check", succeeds=False)
    report = {
        "bundle_id": config["bundle_id"],
        "team_id": config["team_id"],
        "identity_sha1": config["identity_sha1"],
        "designated_requirement_sha256": hashlib.sha256(first_requirement.encode()).hexdigest(),
        "code_hashes": hashes,
        "updated_signed_bundle_version": info["CFBundleVersion"],
        "identical_designated_requirements": True,
        "mutual_requirement_validation": True,
        "ad_hoc_replacement_rejected": True,
        "missing_identity_rejected": True,
        "gui_actions_executed": False,
        "tcc_permission_retention_tested": False,
    }
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print("Signing continuity and rejection checks passed; no app was launched.")


if __name__ == "__main__":
    main()
