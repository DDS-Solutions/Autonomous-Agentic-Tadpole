"""
Unit tests for verification_contract.py
Ensures that 'unverified = failure' is strictly enforced across all helpers.
"""

import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "execution"))
from verification_contract import Unverified, require_match, require_literal, require_file, Report


class TestVerificationContract(unittest.TestCase):
    def test_require_match_raises_when_target_absent(self):
        """The core rule: could not check == failed. NOT a silent pass."""
        with self.assertRaises(Unverified) as ctx:
            require_match(r"pub async fn delete_agent", "", "C2", "cascade delete")
        self.assertIn("C2-UNVERIFIED", str(ctx.exception))

    def test_require_match_uses_dotall_by_default(self):
        """rustfmt wraps at 100 cols; a single-line pattern must still match across lines."""
        wrapped = (
            "fn is_authorized_operator(payload: &Payload) -> bool {\n"
            "    let ok = matches!(\n"
            "        role,\n"
            "        Role::Admin\n"
            "    ) && payload.user_id == claimed_id;\n"
            "    ok\n"
            "}"
        )
        m = require_match(
            r"is_authorized_operator.*?payload\.user_id",
            wrapped,
            "C5",
            "authz decision"
        )
        self.assertIsNotNone(m)

    def test_require_literal_raises_when_absent(self):
        with self.assertRaises(Unverified) as ctx:
            require_literal("target_symbol", "other_content", "TEST", "symbol presence")
        self.assertIn("TEST-UNVERIFIED", str(ctx.exception))

    def test_require_file_raises_when_file_missing(self):
        with self.assertRaises(Unverified) as ctx:
            require_file(Path("non_existent_file_xyz_123.tmp"), "FILE", "temp file")
        self.assertIn("FILE-UNVERIFIED", str(ctx.exception))

    def test_report_records_crashing_check_as_unverified(self):
        """A check that raises anything at all must not count as a pass."""
        report = Report()

        def exploding_check():
            raise ValueError("unexpected refactor")

        report.record("C1", exploding_check)
        self.assertTrue(report.failed)
        self.assertIn("C1-UNVERIFIED", report.errors[0])

    def test_report_records_unverified_as_failed(self):
        report = Report()

        def absent():
            require_literal("is_authorized_operator", "fn other() {}", "C5", "authz")

        report.record("C5", absent)
        self.assertTrue(report.failed)
        self.assertEqual(report.exit("test"), 1)

    def test_clean_report_exits_zero(self):
        report = Report()
        report.record("C1", lambda: None)
        self.assertFalse(report.failed)
        self.assertEqual(report.exit("test"), 0)


if __name__ == "__main__":
    unittest.main()
