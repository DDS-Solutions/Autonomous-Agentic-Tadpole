"""
@docs ARCHITECTURE:Registry:Skills
### AI Assist Note
Compatibility entry point for the canonical execution annotation inventory.
### 🔍 Debugging & Observability
- **Failure Path**: Missing canonical script or unreadable source files.
- **Telemetry Link**: Search `[type_coverage]` in logs.
"""
import importlib.util
from pathlib import Path

source = Path(__file__).resolve().parents[4] / "execution/skills/lint-and-validate/scripts/type_coverage.py"
spec = importlib.util.spec_from_file_location("tadpole_type_annotations", source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
check_typescript_coverage = module.check_typescript_coverage
check_python_coverage = module.check_python_coverage
main = module.main

if __name__ == "__main__":
    main()

# Metadata: [type_coverage]
