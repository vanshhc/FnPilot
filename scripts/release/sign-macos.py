#!/usr/bin/env python3
"""Sign a relocated app inside-out, notarize it and staple the accepted ticket."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def run(*args):
    subprocess.run(args, check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    args = parser.parse_args()
    app = args.app.resolve()
    identity = os.environ["APPLE_SIGNING_IDENTITY"]
    team = os.environ["APPLE_TEAM_ID"]
    if not identity.startswith("Developer ID Application:"):
        raise SystemExit("Official macOS signing requires Developer ID Application")
    for path in sorted(app.rglob("*"), key=lambda p: len(p.parts), reverse=True):
        if path.is_file() and not path.is_symlink():
            with path.open("rb") as stream:
                magic = stream.read(4)
            if magic in [b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe"]:
                run("codesign", "--force", "--timestamp", "--options", "runtime", "--sign", identity, str(path))
    provenance = app / "Contents/Resources/runtime/provenance.json"
    receipt = json.loads(provenance.read_text())
    receipt["files"] = {name: hashlib.sha256((provenance.parent / name).read_bytes()).hexdigest() for name in receipt["files"]}
    receipt["signing_stage"] = "Developer ID signed embedded code"
    provenance.write_text(json.dumps(receipt, indent=2) + "\n")
    run("codesign", "--force", "--timestamp", "--options", "runtime", "--entitlements", "apps/desktop/src-tauri/entitlements.plist", "--sign", identity, str(app))
    run("codesign", "--verify", "--deep", "--strict", str(app))
    with tempfile.TemporaryDirectory(prefix="fnpilot-notary-") as temporary:
        archive = str(Path(temporary) / "FnPilot.zip")
        run("ditto", "-c", "-k", "--keepParent", str(app), archive)
        # The API key file is provisioned only in the protected native job.
        run("xcrun", "notarytool", "submit", archive, "--key", os.environ["APPLE_API_KEY_PATH"], "--key-id", os.environ["APPLE_API_KEY_ID"], "--issuer", os.environ["APPLE_API_ISSUER"], "--wait", "--timeout", "30m")
    run("xcrun", "stapler", "staple", str(app))
    run("python3", "scripts/release/verify-macos.py", str(app), "--team-id", team)
