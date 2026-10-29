import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("gates", Path(__file__).parents[1] / "release/check-gates.py")
gates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gates)


class ReleaseGateTests(unittest.TestCase):
    def test_missing_native_evidence_or_wrong_revision_cannot_authorize_signing(self):
        receipt = {"source_revision": "reviewed", "reviewer": "test reviewer", "reviewed_at": "test timestamp", "receipts": {}}
        with self.assertRaises(ValueError):
            gates.check(receipt, "reviewed")
        receipt["receipts"] = {name: {"status": "passed", "reference": "test fixture only", "sha256": "a" * 64} for name in ["macos", "windows", "performance", "upgrade"]}
        self.assertTrue(gates.check(receipt, "reviewed"))
        with self.assertRaises(ValueError):
            gates.check(receipt, "different")
        receipt["receipts"]["windows"]["status"] = "pending"
        with self.assertRaises(ValueError):
            gates.check(receipt, "reviewed")
