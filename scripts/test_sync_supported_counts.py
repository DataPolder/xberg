from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
import subprocess

from scripts.sync_supported_counts import (
    Counts,
    TextFile,
    parse_registry,
    synchronize_files,
    tracked_text_files,
    verify_files,
)


REGISTRY = """
static FORMATS: &[FormatEntry] = &[
    FormatEntry {
        extensions: &["txt"],
        mime_type: "text/plain",
        aliases: &[],
    },
    FormatEntry {
        extensions: &["md", "markdown"],
        mime_type: "text/markdown",
        aliases: &["text/x-markdown"],
    },
];
"""


class RegistryTests(unittest.TestCase):
    def test_parse_registry_derives_format_and_unique_extension_counts(self) -> None:
        self.assertEqual(parse_registry(REGISTRY), Counts(formats=2, extensions=3))

    def test_parse_registry_rejects_duplicate_extensions(self) -> None:
        duplicate = REGISTRY.replace('["md", "markdown"]', '["txt", "markdown"]')

        with self.assertRaisesRegex(ValueError, "duplicate extension.*txt"):
            parse_registry(duplicate)

    def test_parse_registry_rejects_malformed_entries(self) -> None:
        malformed = REGISTRY.replace('mime_type: "text/plain",', "")

        with self.assertRaisesRegex(ValueError, "malformed FormatEntry"):
            parse_registry(malformed)


class PublicationTests(unittest.TestCase):
    def _tracked_files(self, root: Path, *paths: str) -> list[TextFile]:
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "add", "--", *paths], cwd=root, check=True)
        return tracked_text_files(root)

    def test_verify_reports_every_stale_publication_phrase(self) -> None:
        files = [
            TextFile(Path("README.md"), "Xberg extracts 101 formats across 115 file extensions.\n"),
            TextFile(Path("Cargo.toml"), 'description = "Document extraction from 101 formats"\n'),
        ]

        errors, advertisements = verify_files(Counts(100, 120), files)

        self.assertEqual(advertisements, 3)
        self.assertEqual(
            errors,
            [
                "README.md:1: advertises 101 formats; registry has 100",
                "README.md:1: advertises 115 file extensions; registry has 120",
                "Cargo.toml:1: advertises 101 formats; registry has 100",
            ],
        )

    def test_verify_ignores_unrelated_format_counts(self) -> None:
        files = [
            TextFile(Path("README.md"), "Xberg extracts 100 formats.\n"),
            TextFile(Path("ATTRIBUTIONS.md"), "Pandoc baselines cover 6 formats.\n"),
            TextFile(Path("migration.md"), "Unstructured supports about 30 formats.\n"),
            TextFile(Path("image.rs"), "JPEG 2000 and JBIG2 formats are decoded.\n"),
        ]

        errors, advertisements = verify_files(Counts(100, 120), files)

        self.assertEqual(errors, [])
        self.assertEqual(advertisements, 1)

    def test_verify_recognizes_supports_file_formats(self) -> None:
        errors, advertisements = verify_files(
            Counts(100, 120),
            [TextFile(Path("tool.py"), 'description = "Extract content. Supports 101 file formats."\n')],
        )

        self.assertEqual(advertisements, 1)
        self.assertEqual(errors, ["tool.py:1: advertises 101 formats; registry has 100"])

    def test_verify_does_not_borrow_context_from_an_adjacent_competitor_claim(self) -> None:
        errors, advertisements = verify_files(
            Counts(100, 120),
            [
                TextFile(
                    Path("comparison.md"),
                    "Xberg supports 100 file formats.\nCompetitor supports 80 file formats.\n",
                )
            ],
        )

        self.assertEqual(errors, [])
        self.assertEqual(advertisements, 1)

    def test_verify_attributes_same_line_format_claims_to_their_own_clause(self) -> None:
        errors, advertisements = verify_files(
            Counts(100, 120),
            [
                TextFile(
                    Path("comparison.md"), "Xberg supports 100 file formats; Competitor supports 80 file formats.\n"
                )
            ],
        )

        self.assertEqual(errors, [])
        self.assertEqual(advertisements, 1)

    def test_verify_attributes_same_line_extension_claims_to_their_own_clause(self) -> None:
        errors, advertisements = verify_files(
            Counts(100, 120),
            [
                TextFile(
                    Path("comparison.md"),
                    "Xberg supports 100 formats across 120 file extensions; Competitor supports 80 file extensions.\n",
                )
            ],
        )

        self.assertEqual(errors, [])
        self.assertEqual(advertisements, 2)

    def test_verify_excludes_historical_and_fixture_categories(self) -> None:
        files = [
            TextFile(Path("README.md"), "Xberg extracts 100 formats.\n"),
            TextFile(Path("CHANGELOG.md"), "Xberg previously extracted 88 formats.\n"),
            TextFile(Path("tests/fixtures/result.md"), "Xberg extracts 77 formats.\n"),
            TextFile(Path("vendor/project/README.md"), "Xberg extracts 66 formats.\n"),
        ]

        errors, advertisements = verify_files(Counts(100, 120), files)

        self.assertEqual(errors, [])
        self.assertEqual(advertisements, 1)

    def test_verify_ignores_tracked_test_scripts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "README.md").write_text("Xberg supports 100 formats.\n", encoding="utf-8")
            test_script = root / "scripts" / "test_counts.py"
            test_script.parent.mkdir()
            test_script.write_text('CLAIM = "Xberg supports 77 formats"\n', encoding="utf-8")

            files = self._tracked_files(root, "README.md", "scripts/test_counts.py")
            errors, advertisements = verify_files(Counts(100, 120), files)

            self.assertEqual(errors, [])
            self.assertEqual(advertisements, 1)

    def test_verify_rejects_a_scan_that_finds_no_advertisements(self) -> None:
        errors, advertisements = verify_files(
            Counts(100, 120),
            [TextFile(Path("README.md"), "No numerical claims here.\n")],
        )

        self.assertEqual(advertisements, 0)
        self.assertEqual(errors, ["no supported-format advertisements found in tracked UTF-8 files"])

    def test_synchronize_updates_claims_without_touching_unrelated_counts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            readme = root / "README.md"
            readme.write_text(
                "Xberg extracts 101 formats across 115 file extensions.\nPandoc baselines cover 6 formats.\n",
                encoding="utf-8",
            )

            changed, advertisements = synchronize_files(
                Counts(100, 120),
                [TextFile(Path("README.md"), readme.read_text(encoding="utf-8"))],
                root,
            )

            self.assertEqual(changed, [Path("README.md")])
            self.assertEqual(advertisements, 2)
            self.assertEqual(
                readme.read_text(encoding="utf-8"),
                "Xberg extracts 100 formats across 120 file extensions.\nPandoc baselines cover 6 formats.\n",
            )

            unchanged, second_advertisements = synchronize_files(
                Counts(100, 120),
                [TextFile(Path("README.md"), readme.read_text(encoding="utf-8"))],
                root,
            )
            self.assertEqual(unchanged, [])
            self.assertEqual(second_advertisements, 2)

    def test_synchronize_ignores_small_extension_counts(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            readme = root / "README.md"
            original = "Xberg supports 100 formats and a helper supports 3 file extensions.\n"
            readme.write_text(original, encoding="utf-8")

            changed, advertisements = synchronize_files(
                Counts(100, 120),
                [TextFile(Path("README.md"), original)],
                root,
            )

            self.assertEqual(changed, [])
            self.assertEqual(advertisements, 1)
            self.assertEqual(readme.read_text(encoding="utf-8"), original)

    def test_synchronize_skips_generated_ownership_and_updates_sources(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generated = root / "README.md"
            generated.write_text(
                "<!-- alef:hash:abc -->\nXberg extracts 101 formats.\n",
                encoding="utf-8",
            )
            source = root / "templates" / "readme" / "root.md"
            source.parent.mkdir(parents=True)
            source.write_text("Xberg extracts 101 formats.\n", encoding="utf-8")
            plugin = root / "plugin" / "package.json"
            plugin.parent.mkdir()
            plugin.write_text('{"description":"Xberg extracts 101 formats."}\n', encoding="utf-8")
            marketplace = root / ".claude-plugin" / "marketplace.json"
            marketplace.parent.mkdir()
            marketplace.write_text('{"description":"Xberg extracts 101 formats."}\n', encoding="utf-8")
            agent_marketplace = root / ".agents" / "plugins" / "marketplace.json"
            agent_marketplace.parent.mkdir(parents=True)
            agent_marketplace.write_text('{"description":"Xberg extracts 101 formats."}\n', encoding="utf-8")

            files = [
                TextFile(Path("README.md"), generated.read_text(encoding="utf-8")),
                TextFile(Path("templates/readme/root.md"), source.read_text(encoding="utf-8")),
                TextFile(Path("plugin/package.json"), plugin.read_text(encoding="utf-8")),
                TextFile(Path(".claude-plugin/marketplace.json"), marketplace.read_text(encoding="utf-8")),
                TextFile(Path(".agents/plugins/marketplace.json"), agent_marketplace.read_text(encoding="utf-8")),
            ]
            changed, advertisements = synchronize_files(Counts(100, 120), files, root)

            self.assertEqual(changed, [Path("templates/readme/root.md")])
            self.assertEqual(advertisements, 5)
            self.assertIn("101 formats", generated.read_text(encoding="utf-8"))
            self.assertIn("100 formats", source.read_text(encoding="utf-8"))
            self.assertIn("101 formats", plugin.read_text(encoding="utf-8"))
            self.assertIn("101 formats", marketplace.read_text(encoding="utf-8"))
            self.assertIn("101 formats", agent_marketplace.read_text(encoding="utf-8"))

    def test_synchronize_rejects_a_symlink_destination_without_touching_its_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "repo"
            root.mkdir()
            outside = base / "outside.md"
            outside.write_text("outside\n", encoding="utf-8")
            (root / "README.md").symlink_to(outside)

            with self.assertRaisesRegex(ValueError, "regular file"):
                synchronize_files(
                    Counts(100, 120),
                    [TextFile(Path("README.md"), "Xberg supports 101 formats.\n")],
                    root,
                )

            self.assertEqual(outside.read_text(encoding="utf-8"), "outside\n")

    def test_synchronize_rejects_paths_outside_the_repository_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "repo"
            root.mkdir()
            outside = base / "outside.md"
            outside.write_text("outside\n", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "outside repository root"):
                synchronize_files(
                    Counts(100, 120),
                    [TextFile(Path("../outside.md"), "Xberg supports 101 formats.\n")],
                    root,
                )

            self.assertEqual(outside.read_text(encoding="utf-8"), "outside\n")

    def test_tracked_text_files_rejects_publication_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target.md"
            target.write_text("Xberg supports 100 formats.\n", encoding="utf-8")
            (root / "README.md").symlink_to(target)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "--", "README.md"], cwd=root, check=True)

            with self.assertRaisesRegex(ValueError, "tracked symlink"):
                tracked_text_files(root)

    def test_tracked_text_files_rejects_worktree_symlink_for_index_regular_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            readme = root / "README.md"
            readme.write_text("Xberg supports 100 formats.\n", encoding="utf-8")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "--", "README.md"], cwd=root, check=True)
            target = root / "target.md"
            target.write_text("Xberg supports 77 formats.\n", encoding="utf-8")
            readme.unlink()
            readme.symlink_to(target)

            with self.assertRaisesRegex(ValueError, "working-tree file is not regular"):
                tracked_text_files(root)

    def test_tracked_text_files_rejects_invalid_utf8_publication_text(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "README.md").write_bytes(b"Xberg\xff")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(["git", "add", "--", "README.md"], cwd=root, check=True)

            with self.assertRaisesRegex(ValueError, "invalid UTF-8"):
                tracked_text_files(root)

    def test_tracked_text_files_skips_known_binary_content(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "image.png").write_bytes(b"\x89PNG\r\n\xff")

            files = self._tracked_files(root, "image.png")

            self.assertEqual(files, [])


if __name__ == "__main__":
    unittest.main()
