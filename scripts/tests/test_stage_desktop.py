import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "stage_desktop", Path(__file__).resolve().parents[1] / "stage-desktop.py"
)
stage_desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage_desktop)


class NodeLicenseTests(unittest.TestCase):
    def test_workspace_links_cannot_reenter_staged_license_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            modules = root / "node_modules"
            package = modules / "package"
            package.mkdir(parents=True)
            installed_license = package / "LICENSE"
            installed_license.write_text("installed package license")
            workspace = root / "workspace"
            output = workspace / "resources" / "licenses" / "node"
            output.mkdir(parents=True)
            (output / "NOTICE").write_text("previous staged output")
            link = modules / "workspace"
            if os.name == "nt":
                subprocess.run(
                    ["cmd", "/c", "mklink", "/J", str(link), str(workspace)],
                    check=True, capture_output=True,
                )
                self.assertTrue(link.is_junction())
            else:
                link.symlink_to(workspace, target_is_directory=True)
            try:
                self.assertEqual(
                    list(stage_desktop.node_license_files(modules)), [installed_license]
                )
            finally:
                if os.name == "nt":
                    link.rmdir()
                else:
                    link.unlink()
