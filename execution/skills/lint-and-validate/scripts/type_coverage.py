"""
@docs ARCHITECTURE:Infrastructure:Execution
### AI Assist Note
Annotation inventory across all discovered source files; not compiler type coverage.
### 🔍 Debugging & Observability
- **Failure Path**: Unreadable files or invalid Python syntax are reported explicitly.
- **Telemetry Link**: Search `[type_coverage]` in logs.
"""
import ast
import json
import os
from pathlib import Path
import re
import sys

EXCLUDED_DIRECTORIES = {"node_modules", ".git", ".tmp", "target", "dist", "build", "coverage",
                        "venv", ".venv", "__pycache__", ".next", ".pytest_cache"}


def source_files(root: Path, suffixes: set[str]) -> list[Path]:
    files = []
    for directory, dirs, names in os.walk(root):
        dirs[:] = sorted(name for name in dirs if name not in EXCLUDED_DIRECTORIES)
        files.extend(Path(directory) / name for name in sorted(names)
                     if Path(name).suffix in suffixes and not name.endswith(".d.ts"))
    return sorted(files)


def inventory(project_path: Path, language: str) -> dict:
    files = source_files(project_path, {".ts", ".tsx"} if language == "typescript" else {".py"})
    stats = {"any_count": 0, "typed_functions": 0, "untyped_functions": 0, "total_functions": 0}
    issues, analyzed = [], 0
    for path in files:
        try:
            content = path.read_text(encoding="utf-8-sig")
            if language == "python":
                tree = ast.parse(content, filename=str(path))
                for node in ast.walk(tree):
                    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        args = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
                        args.extend(arg for arg in (node.args.vararg, node.args.kwarg) if arg)
                        typed = node.returns is not None or any(arg.annotation is not None for arg in args)
                        stats["typed_functions" if typed else "untyped_functions"] += 1
                stats["any_count"] += len(re.findall(r":\s*Any\b", content))
            else:
                # Heuristics do not measure inferred types, comments, overloads, or all TS syntax.
                stats["any_count"] += len(re.findall(r":\s*any\b", content))
                stats["untyped_functions"] += len(re.findall(r"function\s+\w+\s*\([^)]*\)\s*\{", content))
                stats["typed_functions"] += len(re.findall(r"function\s+\w+\s*\([^)]*\)\s*:\s*\w+", content))
            analyzed += 1
        except (OSError, UnicodeError, SyntaxError) as error:
            issues.append(f"[X] {path}: {error}")
    stats["total_functions"] = stats["typed_functions"] + stats["untyped_functions"]
    return {"type": language, "measurement": "annotation heuristic", "files": analyzed,
            "discovered_files": len(files), "stats": stats, "issues": issues,
            "passed": [f"[OK] Read {analyzed} of {len(files)} discovered source files"]}


def check_typescript_coverage(project_path: Path) -> dict:
    return inventory(project_path, "typescript")


def check_python_coverage(project_path: Path) -> dict:
    return inventory(project_path, "python")


def main() -> None:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve(strict=True)
    results = [check_typescript_coverage(root), check_python_coverage(root)]
    print(json.dumps(results, indent=2))
    raise SystemExit(1 if any(result["issues"] for result in results) else 0)


if __name__ == "__main__":
    main()

# Metadata: [type_coverage]
