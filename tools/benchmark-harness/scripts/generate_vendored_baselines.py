"""Generate vendored OCR baselines from PaddleOCR Python and RapidOCR.

Usage:
    uv run tools/benchmark-harness/scripts/generate_vendored_baselines.py
    uv run tools/benchmark-harness/scripts/generate_vendored_baselines.py rapidocr
    uv run tools/benchmark-harness/scripts/generate_vendored_baselines.py --force
"""

import json
import os
import sys
import time
from pathlib import Path

import numpy as np

FIXTURES_DIR = Path(__file__).resolve().parent.parent / "fixtures"
VENDORED_DIR = Path(__file__).resolve().parent.parent / "vendored"
COHORTS_DIR = Path(__file__).resolve().parent.parent / "cohorts"
OCR_IMAGES_COHORT = COHORTS_DIR / "ocr-images-fast.json"

PDF_OCR_FIXTURES = [
    "pdf_image_only_german",
    "pdf_non_searchable",
    "pdf_ocr_rotated_270",
    "pdf_ocr_rotated_90",
    "pdf_ocr_rotated",
    "pdf_ocr_test",
    "pdf_scanned_ocr",
]


def load_ocr_fixture_paths() -> list[Path]:
    """Load the PDF fixtures and the canonical fast OCR image cohort."""
    fixture_paths = [FIXTURES_DIR / f"{name}.json" for name in PDF_OCR_FIXTURES]
    cohort = json.loads(OCR_IMAGES_COHORT.read_text(encoding="utf-8"))
    fixture_paths.extend(FIXTURES_DIR / fixture for fixture in cohort["fixtures"])
    return fixture_paths


def pdf_to_images(pdf_path: str, dpi: int = 300) -> list[np.ndarray]:
    """Convert PDF pages to numpy arrays (RGB, HWC)."""
    import io

    import fitz
    from PIL import Image

    doc = fitz.open(pdf_path)
    images = []
    for page in doc:
        mat = fitz.Matrix(dpi / 72, dpi / 72)
        pix = page.get_pixmap(matrix=mat)
        img = Image.open(io.BytesIO(pix.tobytes("png"))).convert("RGB")
        images.append(np.array(img))
    doc.close()
    return images


def document_to_images(document_path: str) -> list[np.ndarray]:
    """Load PDF pages or raster image frames as RGB numpy arrays."""
    if Path(document_path).suffix.lower() == ".pdf":
        return pdf_to_images(document_path)

    from PIL import Image, ImageSequence

    with Image.open(document_path) as image:
        return [np.array(frame.convert("RGB")) for frame in ImageSequence.Iterator(image)]


def lines_to_markdown(lines: list[str]) -> str:
    """Each OCR text line becomes a markdown paragraph."""
    paragraphs = [line.strip() for line in lines if line.strip()]
    return "\n\n".join(paragraphs) + "\n" if paragraphs else ""


def run_paddleocr_python(document_path: str) -> tuple[str, float]:
    """Run PaddleOCR Python v3.4+ using the predict() API."""
    os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
    from paddleocr import PaddleOCR

    ocr = PaddleOCR(use_textline_orientation=True, lang="en")
    images = document_to_images(document_path)

    start = time.monotonic()
    all_lines: list[str] = []
    for img in images:
        for result in ocr.predict(img):
            rec_texts = result.get("rec_text", [])
            if isinstance(rec_texts, (list, tuple)):
                for t in rec_texts:
                    text = str(t).strip()
                    if text:
                        all_lines.append(text)
    elapsed_ms = (time.monotonic() - start) * 1000

    return lines_to_markdown(all_lines), elapsed_ms


def rapidocr_lines(result: object) -> list[str]:
    """Return recognized text from current or legacy RapidOCR output."""
    texts = getattr(result, "txts", None)
    if texts is not None:
        return [str(text).strip() for text in texts if str(text).strip()]

    legacy_result = result[0] if isinstance(result, tuple) else result
    if not legacy_result:
        return []
    return [str(line[1]).strip() for line in legacy_result if line and len(line) >= 2 and str(line[1]).strip()]


def run_rapidocr(document_path: str) -> tuple[str, float]:
    """Run RapidOCR."""
    try:
        from rapidocr import RapidOCR
    except ImportError:
        from rapidocr_onnxruntime import RapidOCR

    ocr = RapidOCR()
    images = document_to_images(document_path)

    start = time.monotonic()
    all_lines: list[str] = []
    for img in images:
        all_lines.extend(rapidocr_lines(ocr(img)))
    elapsed_ms = (time.monotonic() - start) * 1000

    return lines_to_markdown(all_lines), elapsed_ms


def save_vendored(pipeline_name: str, fixture_name: str, md: str, time_ms: float):
    md_dir = VENDORED_DIR / pipeline_name / "md"
    timing_dir = VENDORED_DIR / pipeline_name / "timing"
    md_dir.mkdir(parents=True, exist_ok=True)
    timing_dir.mkdir(parents=True, exist_ok=True)
    (md_dir / f"{fixture_name}.md").write_text(md)
    (timing_dir / f"{fixture_name}.ms").write_text(f"{time_ms:.1f}\n")


def main():
    pipelines = {
        "paddleocr-python": run_paddleocr_python,
        "rapidocr": run_rapidocr,
    }

    force = "--force" in sys.argv
    args = [a for a in sys.argv[1:] if not a.startswith("--")]

    if args:
        selected = args[0]
        if selected not in pipelines:
            print(f"Unknown: {selected}. Choose: {list(pipelines.keys())}")
            sys.exit(1)
        pipelines = {selected: pipelines[selected]}

    for fixture_path in load_ocr_fixture_paths():
        fixture_name = fixture_path.stem
        if not fixture_path.exists():
            print(f"  SKIP {fixture_name}: fixture not found")
            continue

        with open(fixture_path) as f:
            fixture = json.load(f)

        doc_path = str((FIXTURES_DIR / fixture["document"]).resolve())
        if not os.path.exists(doc_path):
            print(f"  SKIP {fixture_name}: document not found")
            continue

        for pipeline_name, run_fn in pipelines.items():
            existing = VENDORED_DIR / pipeline_name / "md" / f"{fixture_name}.md"
            if not force and existing.exists() and existing.stat().st_size > 0:
                print(f"  CACHED {pipeline_name}/{fixture_name}")
                continue

            print(f"  RUN {pipeline_name}/{fixture_name} ...", end="", flush=True)
            try:
                md, time_ms = run_fn(doc_path)
                save_vendored(pipeline_name, fixture_name, md, time_ms)
                print(f" {time_ms:.0f}ms, {len(md)} chars")
            except Exception as e:
                print(f" ERROR: {e}")
                import traceback

                traceback.print_exc()


if __name__ == "__main__":
    main()
