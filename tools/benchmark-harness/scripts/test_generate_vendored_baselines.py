import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import generate_vendored_baselines as baselines


class VendoredBaselineTests(unittest.TestCase):
    def test_load_ocr_fixture_paths_includes_image_cohort(self):
        names = [path.name for path in baselines.load_ocr_fixture_paths()]

        self.assertEqual(len(names), 13)
        self.assertIn("pdf_ocr_test.json", names)
        self.assertIn("image_ocr_test_original.json", names)
        self.assertIn("tif_ocr.json", names)

    def test_document_to_images_loads_all_tiff_frames_as_rgb(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            image_path = Path(temporary_directory) / "multipage.tiff"
            first = Image.new("L", (3, 2), color=10)
            second = Image.new("L", (3, 2), color=20)
            first.save(image_path, save_all=True, append_images=[second])

            frames = baselines.document_to_images(str(image_path))

        self.assertEqual(len(frames), 2)
        self.assertEqual(frames[0].shape, (2, 3, 3))
        self.assertTrue(np.all(frames[1] == 20))

    def test_rapidocr_lines_supports_current_output(self):
        result = SimpleNamespace(txts=(" first ", "", "second"))

        self.assertEqual(baselines.rapidocr_lines(result), ["first", "second"])

    def test_rapidocr_lines_supports_legacy_output(self):
        result = ([[[0, 0], " first ", 0.9], [[0, 0], "", 0.8]], {})

        self.assertEqual(baselines.rapidocr_lines(result), ["first"])


if __name__ == "__main__":
    unittest.main()
