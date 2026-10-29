#!/usr/bin/env python3
"""Relocate Swift runtimes and sign a preview with ad-hoc or explicit development identity."""
import argparse
import plistlib
import shutil
import subprocess
from pathlib import Path


def sign_preview(app, identity="-"):
    if identity != "-" and not identity.startswith("Apple Development:"):
        raise ValueError("Preview signing accepts an explicitly selected Apple Development identity; official releases use the separate release workflow.")
    base = ["codesign", "--force", "--sign", identity, "--timestamp=none"]
    for path in (app / "Contents/Frameworks").glob("*.dylib"):
        subprocess.run([*base, str(path)], check=True, capture_output=True)
    command = list(base)
    if identity != "-":
        entitlements = Path(__file__).resolve().parents[1] / "apps/desktop/src-tauri/entitlements.plist"
        command.extend(["--entitlements", str(entitlements)])
    # Sign the bundle last. Never recursively re-sign the pinned helper: that
    # would change its bytes without updating its provenance inventory.
    subprocess.run([*command, str(app)], check=True, capture_output=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True, capture_output=True)


def relocate(app, identity="-"):
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    binary = app / "Contents/MacOS" / info["CFBundleExecutable"]
    frameworks = app / "Contents/Frameworks"
    frameworks.mkdir(exist_ok=True)
    tool = Path(subprocess.check_output(["xcrun", "--find", "swiftc"], text=True).strip())
    sources = [tool.parent.parent / "lib/swift/macosx", *sorted((tool.parent.parent / "lib").glob("swift-*/macosx"))]
    # Locate every referenced Swift dylib and its transitive Swift dependencies.
    pending = [binary]
    seen = set()
    while pending:
        path = pending.pop()
        deps = subprocess.check_output(["otool", "-L", str(path)], text=True).splitlines()[1:]
        for line in deps:
            name = line.strip().split(" (", 1)[0]
            if not name.startswith("@rpath/libswift"):
                continue
            filename = Path(name).name
            if filename in seen:
                continue
            seen.add(filename)
            source = next((directory / filename for directory in reversed(sources) if (directory / filename).is_file()), None)
            if source is None:
                # OS-provided libraries have no file in modern dyld shared caches.
                # Check whether the loader can resolve the system path instead of guessing.
                import ctypes
                try:
                    ctypes.CDLL("/usr/lib/swift/" + filename)
                except OSError:
                    raise RuntimeError(f"Missing redistributable Swift runtime: {filename}")
                continue
            destination = frameworks / filename
            shutil.copy2(source, destination)
            pending.append(destination)
    subprocess.run(["install_name_tool", "-add_rpath", "@executable_path/../Frameworks", str(binary)], check=False, capture_output=True)
    files = [binary, *frameworks.glob("*.dylib")]
    for path in files:
        lines = subprocess.check_output(["otool", "-l", str(path)], text=True).splitlines()
        for i, line in enumerate(lines):
            if line.strip() == "cmd LC_RPATH":
                rpath = lines[i + 2].strip().split(" ")[1]
                if ".xctoolchain/" in rpath or "/Users/" in rpath:
                    subprocess.run(["install_name_tool", "-delete_rpath", rpath, str(path)], check=True)
    sign_preview(app, identity)
    kind = "ad-hoc" if identity == "-" else "development-signed"
    print(f"Local {kind} preview ready: {app}. This is not an official signed release.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("--identity", default="-", help="Explicit Apple Development identity; defaults to ad-hoc without using Keychain credentials")
    args = parser.parse_args()
    relocate(args.app, args.identity)
