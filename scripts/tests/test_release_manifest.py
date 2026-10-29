import hashlib
import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("artifacts", Path(__file__).parents[1] / "release/verify-artifacts.py")
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


class ReleaseTests(unittest.TestCase):
    def test_missing_helpers_unexpected_binaries_and_wrong_revision_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            entries = []
            for role in ["app", "cli", "runtime", "notices", "sbom"]:
                path = root / role
                path.write_bytes(role.encode())
                entries.append({"path": role, "role": role, "sha256": hashlib.sha256(role.encode()).hexdigest(), "size_bytes": len(role)})
            manifest = {"schema_version": 1, "source_revision": "revision", "engine_revisions": {"llama.cpp": "pinned"}, "dependency_inventory": "sbom", "files": entries, "official": False}
            self.assertTrue(artifacts.verify(manifest, root, "revision"))
            with self.assertRaises(ValueError):
                artifacts.verify(manifest, root, "other")
            (root / "unexpected.exe").write_bytes(b"unexpected")
            with self.assertRaises(ValueError):
                artifacts.verify(manifest, root, "revision")
            (root / "unexpected.exe").unlink()
            (root / "runtime").unlink()
            with self.assertRaises(ValueError):
                artifacts.verify(manifest, root, "revision")
            (root / "runtime").write_bytes(b"runtime")
            manifest["official"] = True
            with self.assertRaises(ValueError):
                artifacts.verify(manifest, root, "revision")
