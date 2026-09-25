import tempfile
import unittest
from pathlib import Path

from audit_release_sources import notice_files, source_row


class SourceInventoryTests(unittest.TestCase):
    def test_notice_files_are_top_level_and_hashes_are_stable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "LICENSE-MIT").write_bytes(b"license text\n")
            (root / "NOTICES.md").write_bytes(b"attribution\n")
            (root / "notes.txt").write_bytes(b"not a notice")
            (root / "nested").mkdir()
            (root / "nested" / "LICENSE").write_bytes(b"nested")
            names = [path.name for path in notice_files(root)]
            self.assertEqual(names, ["LICENSE-MIT", "NOTICES.md"])
            manifest = root / "Cargo.toml"
            manifest.write_text("", encoding="utf-8")
            row = source_row(
                {"name": "example", "version": "1.0", "declared": "MIT", "selected": "MIT"},
                {("example", "1.0"): {"manifest_path": str(manifest),
                                       "source": "registry+https://example.invalid"}},
            )
            self.assertFalse(row["missing_text"])
            self.assertEqual(row["texts"][0]["sha256"],
                             "195dcf5a72d45bbdab0460fa1aaa4feec43308b9a5c90a19da07005eb475d6e7")

    def test_missing_text_is_explicit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "Cargo.toml"
            manifest.write_text("", encoding="utf-8")
            row = source_row(
                {"name": "example", "version": "1.0", "declared": "MIT", "selected": "MIT"},
                {("example", "1.0"): {"manifest_path": str(manifest),
                                       "source": "registry+https://example.invalid"}},
            )
            self.assertTrue(row["missing_text"])
            self.assertEqual(row["texts"], [])

    def test_explicit_iced_vendor_is_inventoried_with_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "vendor" / "iced_tiny_skia"
            (source / "src").mkdir(parents=True)
            (source / "Cargo.toml").write_text(
                '[package]\nname = "iced_tiny_skia"\nversion = "0.14.1"\nlicense = "MIT"\n',
                encoding="utf-8",
            )
            (source / "LICENSE").write_text("MIT license\n", encoding="utf-8")
            (source / "BYAKKO-PATCH.md").write_text("Upstream provenance and local patch.\n", encoding="utf-8")
            (source / "src" / "lib.rs").write_text("// vendored source\n", encoding="utf-8")
            row = source_row(
                {"name": "iced_tiny_skia", "version": "0.14.1", "declared": "MIT", "selected": "MIT"},
                {("iced_tiny_skia", "0.14.1"): {
                    "manifest_path": str(source / "Cargo.toml"), "source": None}},
                root,
            )
            self.assertEqual(row["source"], "vendored")
            self.assertFalse(row["missing_text"])
            self.assertEqual(row["provenance"]["record"], "BYAKKO-PATCH.md")
            self.assertIn("src/lib.rs", [item["path"] for item in row["provenance"]["files"]])

    def test_arbitrary_local_dependency_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "Cargo.toml"
            manifest.write_text("", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Unexpected source"):
                source_row(
                    {"name": "example", "version": "1.0", "declared": "MIT", "selected": "MIT"},
                    {("example", "1.0"): {"manifest_path": str(manifest), "source": None}},
                    root,
                )


if __name__ == "__main__":
    unittest.main()
