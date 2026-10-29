import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "relocate-swift-runtime.py"
SPEC = importlib.util.spec_from_file_location("preview_signing", SCRIPT)
SIGNING = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SIGNING)


class PreviewSigningTests(unittest.TestCase):
    def test_default_uses_no_signing_credentials_and_never_resigns_helper(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(SIGNING.subprocess, "run") as run:
            app = Path(directory) / "Preview.app"
            frameworks = app / "Contents/Frameworks"
            frameworks.mkdir(parents=True)
            (frameworks / "libswift_test.dylib").touch()
            SIGNING.sign_preview(app)
            calls = [call.args[0] for call in run.call_args_list]
            self.assertEqual(calls[0][calls[0].index("--sign") + 1], "-")
            self.assertFalse(any("runtime/llama-server" in str(command) for command in calls))
            self.assertNotIn("--deep", calls[-2])
            self.assertEqual(calls[-1], ["codesign", "--verify", "--deep", "--strict", str(app)])

    def test_development_identity_is_explicit_and_keeps_audio_entitlement(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(SIGNING.subprocess, "run") as run:
            app = Path(directory) / "Preview.app"
            SIGNING.sign_preview(app, "Apple Development: Fixture (TEST)")
            command = run.call_args_list[0].args[0]
            self.assertIn("Apple Development: Fixture (TEST)", command)
            self.assertIn("--entitlements", command)
            self.assertIn("--timestamp=none", command)

    def test_release_or_unknown_identity_cannot_use_preview_signing(self):
        with patch.object(SIGNING.subprocess, "run") as run:
            with self.assertRaises(ValueError):
                SIGNING.sign_preview(Path("Preview.app"), "Developer ID Application: Fixture")
            run.assert_not_called()
