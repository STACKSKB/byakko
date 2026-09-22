import unittest
from check_dependency_licenses import parse_tree, selection, FONT_EXPRESSION


class LicenseGateTests(unittest.TestCase):
    def test_alternative_does_not_select_gpl(self):
        self.assertEqual(selection("self_cell", "Apache-2.0 OR GPL-2.0-only"), "Apache-2.0")
        for expression in ("GPL-2.0-only", "MIT AND GPL-2.0-only", "", "New-License"):
            with self.assertRaises(ValueError):
                selection("example", expression)

    def test_asset_exception_is_package_specific(self):
        self.assertIn("font assets", selection("epaint_default_fonts", FONT_EXPRESSION))
        with self.assertRaises(ValueError):
            selection("some-code", FONT_EXPRESSION)

    def test_tree_deduplicates_and_fails_closed(self):
        rows = parse_tree("byakko v0.1.0 (C:\\project)|\nexample v1.0.0|MIT\nexample v1.0.0|MIT (*)\n")
        self.assertEqual(len(rows), 1)
        for text in ("", "not a package|MIT", "a v1|MIT\na v1|ISC"):
            with self.assertRaises(ValueError):
                parse_tree(text)


if __name__ == "__main__":
    unittest.main()
