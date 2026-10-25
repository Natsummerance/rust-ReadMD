"""Pure adversarial controls for the independent Windows fault inspector.

No product, engine, observer or any other process is started; these controls
exercise only the deterministic helpers in inspect-windows-faults.py.
"""
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("windows_faults", HERE / "inspect-windows-faults.py")
qa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qa)


class TileGeometryControls(unittest.TestCase):
    def test_geometry_is_fixed_and_rejects_every_other_shape(self):
        self.assertTrue(qa.valid_tile_geometry(3000, 3500))
        for width, height in [(3000, 3501), (2999, 3500), (0, 0), (1000, 170), (3500, 3000)]:
            with self.subTest(shape=(width, height)):
                with self.assertRaises(AssertionError):
                    qa.valid_tile_geometry(width, height)
        for width, height in [(3000.0, 3500), ("3000", 3500), (3000, None)]:
            with self.subTest(type=(width, height)):
                with self.assertRaises(AssertionError):
                    qa.valid_tile_geometry(width, height)

    def test_source_digest_is_pinned_to_the_locked_repository_probe(self):
        self.assertEqual(qa.PROBE_SHA256, "b978c564d7f48e172d6bf3fa52f18cb81938f69348dbf531165ca68ca6150b29")
        self.assertEqual(len(qa.PROBE_SHA256), 64)
        self.assertEqual((qa.PROBE_WIDTH, qa.PROBE_HEIGHT), (1000, 170))

    def test_fixed_summary_is_pure_and_deterministic(self):
        source, output = b"probe-bytes", b"derived-bytes"
        first = qa.fixed_summary(source, output, 3000, 3500)
        second = qa.fixed_summary(source, output, 3000, 3500)
        self.assertEqual(first, second)
        self.assertEqual(first["sourceSha256"], hashlib.sha256(source).hexdigest())
        self.assertEqual(first["outputSha256"], hashlib.sha256(output).hexdigest())
        self.assertEqual((first["width"], first["height"]), (3000, 3500))
        self.assertTrue(first["sourcePixelsPreserved"])
        with self.assertRaises(AssertionError):
            qa.fixed_summary(source, output, 3000, 3499)
        with self.assertRaises(AssertionError):
            qa.fixed_summary(b"", output, 3000, 3500)
        with self.assertRaises(AssertionError):
            qa.fixed_summary(source, bytearray(qa.MAX_MATERIAL_BYTES + 1), 3000, 3500)
        # Changing one pixel byte must change the fixed output digest.
        changed = qa.fixed_summary(source, b"derived-byteS", 3000, 3500)
        self.assertNotEqual(changed["outputSha256"], first["outputSha256"])


class OcrTextControls(unittest.TestCase):
    def test_requires_both_tokens_in_real_recognition_text(self):
        good = "# READMD scan\n\n行 12345 识别\n"
        result = qa.validate_ocr_text(good)
        self.assertTrue(result["passed"])
        self.assertEqual(result["tokens"], {"READMD": True, "12345": True})

    def test_rejects_each_missing_token_and_separated_digits(self):
        for bad in ["READMD only", "12345 only", "", "READMD 1234", "READMD1234X"]:
            with self.subTest(text=bad):
                with self.assertRaises(AssertionError):
                    qa.validate_ocr_text(bad)

    def test_whitespace_may_separate_a_token_but_letters_may_not(self):
        self.assertTrue(qa.validate_ocr_text("R E A D M D\n12 3 45")["passed"])
        with self.assertRaises(AssertionError):
            qa.validate_ocr_text("R.E.A.D.M.D 1-2-3-4-5")
        with self.assertRaises(AssertionError):
            qa.validate_ocr_text(12345)

    def test_non_text_input_is_rejected(self):
        for bad in [None, 12345, b"READMD 12345", ["READMD", "12345"]]:
            with self.subTest(value=bad):
                with self.assertRaises(AssertionError):
                    qa.validate_ocr_text(bad)


class CliControls(unittest.TestCase):
    def test_tile_refuses_an_unknown_source_digest_before_writing(self):
        with tempfile.TemporaryDirectory(prefix="readmd-faults-pure-") as raw:
            base = Path(raw)
            source = base / "probe.png"
            source.write_bytes(b"not the locked probe image")
            output = base / "derived.png"
            with self.assertRaises(AssertionError):
                qa.tile(argparse_stub(source, output, 3000, 3500))
            self.assertFalse(output.exists())

    def test_tile_refuses_drifted_geometry(self):
        with tempfile.TemporaryDirectory(prefix="readmd-faults-pure-") as raw:
            base = Path(raw)
            source = base / "probe.png"
            source.write_bytes(b"placeholder")
            with self.assertRaises(AssertionError):
                qa.tile(argparse_stub(source, base / "derived.png", 4000, 3500))


def argparse_stub(source, output, width, height):
    class Args:
        pass
    args = Args()
    args.source, args.output, args.width, args.height = str(source), str(output), width, height
    return args


class JsonShapeControls(unittest.TestCase):
    def test_helpers_produce_json_serializable_shapes(self):
        summary = qa.fixed_summary(b"a", b"b", 3000, 3500)
        text = qa.validate_ocr_text("READMD 12345")
        json.dumps(summary, allow_nan=False)
        json.dumps(text, allow_nan=False)
        self.assertIn("sourceSha256", summary)
        self.assertIn("tokens", text)


if __name__ == "__main__":
    unittest.main()
