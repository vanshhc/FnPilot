import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("baseline", Path(__file__).parents[1] / "validate-baseline.py")
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    def test_incomplete_measurement_cannot_pass(self):
        import hashlib
        report = {"corpus_sha256": hashlib.sha256(b"corpus").hexdigest(), "revision": "test", "configuration": {"model": None}, "measurement_status": "measured"}
        with self.assertRaises(ValueError):
            baseline.validate_report(report, b"corpus")
        report.update(paired_runs=[1, 2, 3], hardware="declared machine")
        baseline.validate_report(report, b"corpus")
        with self.assertRaises(ValueError):
            baseline.validate_report(report, b"changed")
