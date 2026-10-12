"""
@docs ARCHITECTURE:Core
### AI Assist Note
Regression tests for complete annotation inventories and truthful advisory reporting.
### 🔍 Debugging & Observability
- **Failure Path**: Silent sample truncation, swallowed errors, or false audit pass.
- **Telemetry Link**: Search `[audit_tool_reporting]` in test output.
"""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(relative):
    spec = importlib.util.spec_from_file_location(Path(relative).stem, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


audit = load(".agent/skills/vulnerability-scanner/scripts/dependency_analyzer.py")
types = load("execution/skills/lint-and-validate/scripts/type_coverage.py")
security = load(".agent/skills/vulnerability-scanner/scripts/security_scan.py")


class AuditReportingTests(unittest.TestCase):
    def test_cargo_advisories_are_parsed_and_unknown_severity_is_high(self):
        data = {"vulnerabilities": {"count": 1, "list": [{
            "advisory": {"id": "RUSTSEC-fixture", "title": "Fixture advisory"},
            "package": {"name": "fixture", "version": "1.0.0"}}]}}
        finding = audit.parse_cargo_audit(data)[0]
        self.assertEqual(finding["severity"], "high")
        self.assertEqual(finding["id"], "RUSTSEC-fixture")
        data["vulnerabilities"]["count"] = 2
        with self.assertRaises(ValueError):
            audit.parse_cargo_audit(data)

    def test_invalid_advisory_reports_do_not_pass(self):
        for data in ({}, {"error": {"code": "unavailable"}},
                     {"metadata": {"vulnerabilities": {"high": -1}}}):
            with self.assertRaises(ValueError):
                audit.parse_npm_audit(data)

    def test_cargo_soundness_and_maintenance_warnings_are_not_discarded(self):
        warning = {"advisory": {"id": "RUSTSEC-warning", "title": "Fixture warning"},
                   "package": {"name": "fixture", "version": "1.0.0"}}
        data = {"vulnerabilities": {"count": 0, "list": []},
                "warnings": {"unsound": [warning], "unmaintained": [warning]}}
        findings = audit.parse_cargo_audit(data)
        self.assertEqual([f["severity"] for f in findings], ["high", "low"])
        self.assertEqual(findings[0]["kind"], "unsound")
        data["warnings"]["unsound"] = {}
        with self.assertRaises(ValueError):
            audit.parse_cargo_audit(data)

    def test_missing_audit_tools_are_unverified(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "package.json").write_text("{}")
            (root / "package-lock.json").write_text("{}")
            with patch.object(audit.shutil, "which", return_value=None):
                report = audit.analyze_dependencies(directory)
            self.assertEqual(report["status"], "unverified")

    def test_compatibility_scanner_uses_actual_managers_and_unverified_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "package.json").write_text("{}")
            (root / "package-lock.json").write_text("{}")
            report = security.run_full_scan(directory, "deps", offline=True)
            self.assertEqual(report["summary"]["overall_status"], "unverified")
            self.assertEqual(report["summary"]["total_findings"], 0)
            self.assertEqual(list(report["scans"]["dependencies"]["audits"]), ["npm", "cargo"])

    def test_compatibility_patterns_distinguish_function_names_from_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "calibration.py").write_text("def calibration_eval():\n    return {}\n")
            report = security.run_full_scan(directory, "patterns")
            self.assertEqual(report["summary"]["overall_status"], "pass")
            (root / "danger.py").write_text("result = eval(untrusted)\n")
            report = security.run_full_scan(directory, "patterns")
            self.assertEqual(report["summary"]["overall_status"], "fail")
            self.assertEqual(report["summary"]["critical"], 1)

    def test_missing_framework_configuration_and_invalid_paths_do_not_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(security.run_full_scan(directory, "config")["summary"]["overall_status"], "unverified")
            with self.assertRaises(FileNotFoundError):
                security.run_full_scan(str(Path(directory) / "missing"))

    def test_unreadable_security_source_is_unverified(self):
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory) / "unreadable.py").write_text("value = 1\n")
            with patch.object(Path, "read_text", side_effect=PermissionError("fixture denied")):
                report = security.run_full_scan(directory, "patterns")
            self.assertEqual(report["summary"]["overall_status"], "unverified")
            self.assertEqual(len(report["scans"]["code_patterns"]["read_errors"]), 1)

    def test_inventory_reads_beyond_thirty_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for index in range(35):
                (root / f"{index:02}.ts").write_text("const value: any = 1;" if index == 34 else "const value = 1;")
            (root / "node_modules").mkdir()
            (root / "node_modules/ignored.ts").write_text("const value: any = 1;")
            result = types.check_typescript_coverage(root)
            self.assertEqual(result["files"], 35)
            self.assertEqual(result["stats"]["any_count"], 1)
            self.assertEqual(result["measurement"], "annotation heuristic")

    def test_python_annotations_are_counted_once(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "fixture.py").write_text("def typed(x: int) -> int:\n    return x\n\ndef untyped(x):\n    return x\n")
            stats = types.check_python_coverage(root)["stats"]
            self.assertEqual(stats["typed_functions"], 1)
            self.assertEqual(stats["untyped_functions"], 1)

    def test_unreadable_input_is_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "invalid.ts").write_bytes(b"\xff")
            result = types.check_typescript_coverage(root)
            self.assertEqual(result["files"], 0)
            self.assertEqual(result["discovered_files"], 1)
            self.assertEqual(len(result["issues"]), 1)


if __name__ == "__main__":
    unittest.main()
