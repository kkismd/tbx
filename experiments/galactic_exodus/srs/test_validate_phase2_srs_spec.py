from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from experiments.galactic_exodus.archive.evaluation.srs import validate_phase2_srs_spec as validator


class Phase2SrsSpecValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tempdir.name) / "phase2_srs_spec.md"
        source = (
            Path(__file__).resolve().parents[1]
            / "docs"
            / "archive"
            / "phase2_srs_spec.md"
        )
        self.text = source.read_text(encoding="utf-8")
        self.write()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def write(self) -> None:
        self.path.write_text(self.text, encoding="utf-8")

    def test_valid_spec_is_accepted(self) -> None:
        validator.validate(self.path)

    def test_tbd_is_rejected(self) -> None:
        self.text += "\nTBD\n"
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "must not contain TBD"):
            validator.validate(self.path)

    def test_follow_up_issue_reference_is_required(self) -> None:
        self.text = self.text.replace("#1167", "")
        self.write()
        with self.assertRaisesRegex(validator.ValidationError, "#1167"):
            validator.validate(self.path)


if __name__ == "__main__":
    unittest.main()
