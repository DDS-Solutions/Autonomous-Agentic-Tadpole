"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Parity Guard Skill**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
"""

from typing import Dict, Any
from core.base_skill import BaseSkill
from pydantic import Field
import os
import sys
import json
from pathlib import Path

class ParityGuardSkill(BaseSkill):
    """
    Audits the Tadpole OS infrastructure for parity between code, docs, and manifests.
    """
    name: str = "parity_guard"
    description: str = "Checks for drift between Axum routes, OpenAPI specs, environment variables, and skill manifests."
    category: str = "integrity"

    class Arguments(BaseSkill.Arguments):
        fix: bool = Field(default=False, description="Whether to attempt automatic fixes (e.g., syncing API_REFERENCE.md).")

    async def execute(self, args: Dict[str, Any]) -> str:
        fix_mode = args.get("fix", False)
        
        workspace_root = os.environ.get("WORKSPACE_ROOT", os.getcwd())
        self.log(f"Starting parity audit (fix={fix_mode})")
        
        # We'll import the logic from the script
        script_path = Path(workspace_root) / "execution" / "parity_guard.py"
        
        try:
            import importlib.util
            spec = importlib.util.spec_from_file_location("parity_guard", str(script_path))
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            
            # Since parity_guard prints results to stdout, and we captured it in registry.py,
            # we can just call its check_parity function.
            # However, we want to return the result as a string.
            
            import io
            from contextlib import redirect_stdout
            
            output = io.StringIO()
            with redirect_stdout(output):
                success = module.check_parity(workspace_root, fix=fix_mode)
            
            result_text = output.getvalue()
            status = "PASSED" if success else "FAILED"
            
            return f"--- PARITY AUDIT {status} ---\n\n{result_text}"

        except Exception as e:
            return f"Error during parity audit: {str(e)}"

# Metadata: [parity_guard_skill]

# Metadata: [parity_guard_skill]
