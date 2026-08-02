import json
import sys
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np
from PIL import Image

# ~keep: This script intentionally uses unittest so it can run without pytest.
# ruff: noqa: D101, D102, PT009, PT027

sys.path.insert(0, str(Path(__file__).resolve().parent))
import generate_vendored_baselines as baselines


class VendoredBaselineTests(unittest.TestCase):
    def test_load_ocr_fixture_paths_includes_image_cohort(self):
        names = [path.name for path in baselines.load_ocr_fixture_paths()]

        self.assertEqual(
            names,
            [
                *(f"{name}.json" for name in baselines.PDF_OCR_FIXTURES),
                "cord_receipt_01.json",
                "cord_receipt_02.json",
                "cord_receipt_03.json",
                "cord_receipt_04.json",
                "doclaynet_page_01.json",
                "doclaynet_page_02.json",
                "ndl_meiji_vertical_01.json",
                "ndl_meiji_vertical_02.json",
                "ndl_meiji_vertical_03.json",
                "ndl_meiji_vertical_04.json",
                "ndl_meiji_vertical_05.json",
                "textocr_scene_01.json",
                "textocr_scene_02.json",
                "textocr_scene_03.json",
            ],
        )

    def test_load_ocr_fixture_paths_filters_exact_category_in_filename_order(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            fixtures_dir = Path(temporary_directory)
            nested_dir = fixtures_dir / "nested"
            nested_dir.mkdir()
            fixtures = {
                "z_match.json": {"metadata": {"category": "image-ocr-realgt"}},
                "a_other.json": {"metadata": {"category": "image-ocr"}},
                "nested/b_match.json": {"metadata": {"category": "image-ocr-realgt"}},
                "c_missing.json": {"metadata": {}},
            }
            for name, fixture in fixtures.items():
                (fixtures_dir / name).write_text(json.dumps(fixture), encoding="utf-8")

            with patch.object(baselines, "FIXTURES_DIR", fixtures_dir):
                paths = baselines.load_ocr_fixture_paths("image-ocr-realgt")

        self.assertEqual(
            [path.relative_to(fixtures_dir).as_posix() for path in paths],
            ["nested/b_match.json", "z_match.json"],
        )

    def test_default_fixture_selection_deduplicates_preserving_order(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            fixtures_dir = root / "fixtures"
            fixtures_dir.mkdir()
            cohort_path = root / "cohort.json"
            cohort_path.write_text(
                json.dumps({"fixtures": ["a.json", "b.json", "z.json"]}),
                encoding="utf-8",
            )

            with ExitStack() as patches:
                patches.enter_context(patch.object(baselines, "FIXTURES_DIR", fixtures_dir))
                patches.enter_context(patch.object(baselines, "OCR_IMAGES_COHORT", cohort_path))
                patches.enter_context(patch.object(baselines, "PDF_OCR_FIXTURES", ["z", "a", "z"]))
                paths = baselines.load_ocr_fixture_paths()

        self.assertEqual([path.name for path in paths], ["z.json", "a.json", "b.json"])

    def test_parse_args_preserves_pipeline_and_force_and_accepts_category(self):
        args = baselines.parse_args(["rapidocr", "--force", "--category", "image-ocr-realgt"])

        self.assertEqual(args.pipeline, "rapidocr")
        self.assertTrue(args.force)
        self.assertEqual(args.category, "image-ocr-realgt")

        force_only_args = baselines.parse_args(["--force"])
        self.assertIsNone(force_only_args.pipeline)
        self.assertTrue(force_only_args.force)

    def test_resolve_document_path_uses_fixture_directory(self):
        fixture_path = Path("fixtures/nested/example.json")

        document_path = baselines.resolve_document_path(fixture_path, {"document": "../document.png"})

        self.assertEqual(document_path, (Path("fixtures") / "document.png").resolve())

    def test_validate_unique_fixture_names_rejects_output_collisions(self):
        fixture_paths = [Path("first/example.json"), Path("second/example.json")]

        with self.assertRaisesRegex(ValueError, "duplicate output names: example"):
            baselines.validate_unique_fixture_names(fixture_paths)

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
