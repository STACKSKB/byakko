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


if __name__ == "__main__":
    unittest.main()
