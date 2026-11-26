"""Tests for check_book.py: python3 -m unittest discover -s scripts -p 'test_*.py'

Tests written by Claude.
"""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_book as cb  # noqa: E402

SUMMARY = """# Summary

[Introduction](introduction.md)

- [Install](install.md)
- [Still to write]()
"""


class Book(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name)
        self.write("book/src/SUMMARY.md", SUMMARY)
        self.write("book/src/introduction.md", "# Hi\n\n![shot](images/a.png)\n[install](install.md)\n")
        self.write("book/src/install.md", "# Install\n\n[back](introduction.html#top) [web](https://x.org)\n")
        self.write("book/src/images/a.png", "png")
        self.write("README.md", "[book](book/src/SUMMARY.md) ![s](book/src/images/a.png)\n")

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, path, text):
        f = self.repo / path
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(text)

    def test_a_sound_book_has_no_problems(self):
        self.assertEqual(cb.check(self.repo), [])

    def test_a_missing_image_or_page_is_named_with_its_line(self):
        self.write("book/src/install.md", "# Install\n\n![gone](images/gone.png)\n[x](nope.md#a)\n")
        self.assertEqual(
            cb.check(self.repo),
            [
                "book/src/install.md:3: images/gone.png isn't there",
                "book/src/install.md:4: nope.md#a isn't there",
            ],
        )

    def test_a_chapter_in_summary_must_be_there(self):
        self.write("book/src/SUMMARY.md", SUMMARY + "- [Gone](gone.md)\n")
        self.assertIn("book/src/SUMMARY.md:7: gone.md isn't there", cb.check(self.repo))

    def test_a_page_not_in_summary_is_found(self):
        self.write("book/src/dev/stray.md", "# Stray\n")
        self.assertEqual(cb.check(self.repo), ["book/src/dev/stray.md: not in SUMMARY.md"])

    def test_the_readme_is_checked_too(self):
        self.write("README.md", "![old](docs/images/a.png)\n")
        self.assertEqual(cb.check(self.repo), ["README.md:1: docs/images/a.png isn't there"])

    def test_links_in_code_arent_links(self):
        self.write(
            "book/src/install.md",
            "# Install\n\n```md\n[a](gone.md)\n```\n`[b](gone.md)` and [c](install.md \"title\")\n",
        )
        self.assertEqual(cb.check(self.repo), [])

    def test_no_summary_is_one_problem(self):
        (self.repo / "book/src/SUMMARY.md").unlink()
        self.assertEqual(cb.check(self.repo), ["book/src/SUMMARY.md isn't there"])


if __name__ == "__main__":
    unittest.main()
