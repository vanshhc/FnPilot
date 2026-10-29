#!/usr/bin/env python3
"""Stage verified helper, nonexecuting CLI, notices and dependency provenance for Tauri."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def node_license_files(directory):
    """Collect installed package licenses without entering workspace links."""
    for path in directory.iterdir():
        if path.is_symlink() or getattr(path, "is_junction", lambda: False)():
            continue
        if path.is_dir():
            yield from node_license_files(path)
        elif path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE")) and path.is_file():
            yield path


def stage():
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
    if sys.platform == "darwin":
        run(sys.executable, "scripts/prepare-whisperkit.py")
        run("swift", "build", "--package-path", "native/macos", "--configuration", "release", "--product", "FnPilotSpeech", "--force-resolved-versions")
    suffix = ".exe" if sys.platform == "win32" else ""
    runtime = ROOT / "Resources/Runtime"
    provenance = runtime / ".runtime-provenance.json"
    expected = json.loads((ROOT / "scripts/runtime-manifest.json").read_text())
    valid = False
    if provenance.exists():
        receipt = json.loads(provenance.read_text())
        valid = receipt.get("revision") == expected["revision"] and bool(receipt.get("files")) and all(
            Path(name).name == name and (runtime / name).is_file()
            and hashlib.sha256((runtime / name).read_bytes()).hexdigest() == digest
            for name, digest in receipt.get("files", {}).items())
    if not valid:
        if sys.platform == "win32":
            run("powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/build-runtime.ps1")
        else:
            run("bash", "scripts/build-runtime.sh")
    run(cargo, "build", "--release", "--locked", "-p", "fnpilot-cli", "-p", "fnpilot-bench")
    resources = ROOT / "apps/desktop/src-tauri/resources"
    if resources.exists():
        shutil.rmtree(resources)
    resources.mkdir(parents=True)
    destination = resources / "runtime"
    destination.mkdir()
    receipt = json.loads(provenance.read_text())
    if receipt["revision"] != expected["revision"]:
        raise RuntimeError("Helper revision mismatch")
    for name, digest in receipt["files"].items():
        if Path(name).name != name or hashlib.sha256((runtime / name).read_bytes()).hexdigest() != digest:
            raise RuntimeError("Helper inventory mismatch")
        shutil.copy2(runtime / name, destination / name, follow_symlinks=True)
    shutil.copy2(provenance, destination / "provenance.json")
    cli = resources / "cli"
    cli.mkdir()
    for name in ["fnpilot-cli", "fnpilot-bench"]:
        shutil.copy2(ROOT / f"target/release/{name}{suffix}", cli / f"{name}{suffix}")
    licenses = resources / "licenses"
    licenses.mkdir()
    for name in ["LICENSE", "THIRD_PARTY_NOTICES.txt"]:
        shutil.copy2(ROOT / name, licenses / name)
    # Include the actual license and notice files shipped by the locked native dependencies.
    target = next(line.split(":",1)[1].strip() for line in subprocess.check_output([cargo, "-vV"], text=True).splitlines() if line.startswith("host:"))
    metadata = json.loads(subprocess.check_output([cargo, "metadata", "--locked", "--offline", "--filter-platform", target, "--format-version", "1"], cwd=ROOT))
    for package in metadata["packages"]:
        source = Path(package["manifest_path"]).parent
        target = licenses / "rust" / f'{package["name"]}-{package["version"]}'
        for path in source.iterdir():
            if path.is_file() and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE")):
                target.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target / path.name)
    for source in node_license_files(ROOT / "node_modules"):
        target = licenses / "node" / source.relative_to(ROOT / "node_modules")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for source in [ROOT / ".build-runtime/source", ROOT / ".build-runtime-windows/source", ROOT / "native/macos/.build/offline-whisperkit", ROOT / "native/macos/.build/checkouts/swift-transformers"]:
        if source.exists():
            for path in source.rglob("*"):
                if path.is_file() and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE")) and not any(part in [".git", ".build"] for part in path.relative_to(source).parts):
                    target = licenses / "native" / source.name / path.relative_to(source)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, target)
    environment = os.environ.copy()
    environment["PATH"] = str(Path(cargo).parent) + os.pathsep + environment.get("PATH", "")
    subprocess.run([sys.executable, "scripts/license-inventory.py", str(resources / "dependencies.json")], cwd=ROOT, env=environment, check=True)
    if sys.platform == "darwin":
        for bundle in (ROOT / "native/macos/.build/release").glob("*.bundle"):
            shutil.copytree(bundle, resources / bundle.name)
    # Registry/package-manager licenses may be read-only. Tauri must be able to replace
    # its previous resource copies on a second build; never change dependency caches.
    for folder in [resources, ROOT / "target/release/licenses", ROOT / "target/release/bundle/macos/FnPilot Preview.app/Contents/Resources/licenses", ROOT / "target/release/bundle/macos/FnPilot.app/Contents/Resources/licenses"]:
        if folder.is_dir():
            for path in folder.rglob("*"):
                if path.is_file() and not path.is_symlink():
                    path.chmod(path.stat().st_mode | 0o200)
    print("Staged CLI, pinned helper, licenses and dependency inventory; no model weights included.")


if __name__ == "__main__":
    stage()
