"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**🛡️ Tadpole Engine: Verification Contract**
Verification contract: a check that did not run is a FAILED check.

### 🔍 Debugging & Observability
- **Failure Path**: Script error, unverified checks, or logic drift in the 3-layer architecture.
- **Telemetry Link**: Search `[verification_contract]` in system logs.

Every gate in execution/ must import `require_*` from here. The rule is
non-negotiable: a script that cannot locate its target, cannot load its
rules, or cannot parse its input MUST exit non-zero. Absence of a check is
never absence of a violation.

Rationale: four independent gates (parity_guard.scan_router,
nexus_adversarial_guard c1/c2/c4/c5/h1, env_schema.validate_and_report,
backup_db.sh) reported success while verifying nothing in legacy versions.
This module enforces that "unverified = failure".
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Sequence

__all__ = [
    "Unverified",
    "require_match",
    "require_literal",
    "require_file",
    "require_env",
    "run_subprocess",
    "Report",
]


class Unverified(Exception):
    """A check could not be executed. Distinct from 'the check failed'."""


def require_match(
    pattern: str, text: str, check_id: str, description: str, *, flags: int = re.DOTALL
) -> re.Match[str]:
    """Locate `pattern` or raise Unverified. Never returns None.

    Defaults to re.DOTALL: rustfmt wraps at 100 columns, so any Rust
    construct that must be matched across lines requires it. Callers that
    genuinely want single-line matching pass flags=re.MULTILINE.
    """
    match = re.search(pattern, text, flags)
    if match is None:
        raise Unverified(
            f"[{check_id}-UNVERIFIED] {description}: target pattern not located. "
            f"The invariant was NOT checked. Pattern: {pattern!r}"
        )
    return match


def require_literal(needle: str, text: str, check_id: str, description: str) -> None:
    """Ensure `needle` exists in `text` or raise Unverified."""
    if needle not in text:
        raise Unverified(
            f"[{check_id}-UNVERIFIED] {description}: literal {needle!r} absent. "
            f"The invariant was NOT checked."
        )


def require_file(path: Path, check_id: str, description: str) -> Path:
    """Ensure `path` exists as a file or raise Unverified."""
    if not path.is_file():
        raise Unverified(
            f"[{check_id}-UNVERIFIED] {description}: file missing at {path}. "
            f"The invariant was NOT checked."
        )
    return path


def require_env(name: str, check_id: str, description: str) -> str:
    """Ensure environment variable `name` is set and non-empty or raise Unverified."""
    value = os.environ.get(name, "").strip()
    if not value:
        raise Unverified(f"[{check_id}-UNVERIFIED] {description}: ${name} unset or empty.")
    return value


def run_subprocess(
    cmd: Sequence[str], check_id: str, description: str, *, cwd: Path | None = None
) -> str:
    """Run a command; raise Unverified on crash, distinct from non-zero exit.

    A crash (traceback on stderr, unhandled signal) must not be read as success.
    """
    proc = subprocess.run(
        list(cmd), cwd=cwd, capture_output=True, text=True, timeout=300, check=False
    )
    if proc.returncode < 0 or "Traceback (most recent call last)" in proc.stderr:
        raise Unverified(
            f"[{check_id}-UNVERIFIED] {description}: {cmd[0]} crashed "
            f"(rc={proc.returncode}).\n{proc.stderr.strip()[:400]}"
        )
    return proc.stdout


@dataclass
class Report:
    """Accumulates failures. `failed` is the ONLY exit signal."""

    errors: list[str] = field(default_factory=list)

    def fail(self, check_id: str, message: str) -> None:
        self.errors.append(f"[{check_id}-FAIL] {message}")

    def unverified(self, check_id: str, message: str) -> None:
        self.errors.append(f"[{check_id}-UNVERIFIED] {message}")

    def record(self, check_id: str, fn, *args, **kwargs) -> None:
        """Run one check, converting Unverified or unhandled exceptions into a recorded failure."""
        try:
            fn(*args, **kwargs)
        except Unverified as exc:
            self.unverified(check_id, str(exc))
        except Exception as exc:  # noqa: BLE001 - a crashing check is not a pass
            self.unverified(check_id, f"check raised unexpectedly: {exc!r}")

    @property
    def failed(self) -> bool:
        return bool(self.errors)

    def exit(self, label: str = "verification") -> int:
        if self.errors:
            msg = f"\n[FAIL] {label}: {len(self.errors)} unresolved violation(s)/unverified check(s)"
            try:
                print(msg)
                for err in self.errors:
                    print(f"  {err}")
            except UnicodeEncodeError:
                print(msg.encode("ascii", "replace").decode("ascii"))
                for err in self.errors:
                    print(f"  {err.encode('ascii', 'replace').decode('ascii')}")
            return 1
        msg = f"\n[OK] {label}: all checks executed and passed"
        try:
            print(msg)
        except UnicodeEncodeError:
            print(msg.encode("ascii", "replace").decode("ascii"))
        return 0

# Metadata: [verification_contract]
