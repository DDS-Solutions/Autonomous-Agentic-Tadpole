"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Verify Context Skill**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
"""

from typing import Dict, Any, List
from core.base_skill import BaseSkill
from pydantic import Field
import os
import sys
import json
from pathlib import Path

class VerifyContextSkill(BaseSkill):
    """
    Audits the workspace for AI Context Alignment (AI Assist Notes, Debugging sections, etc.).
    """
    name: str = "verify_ai_context"
    description: str = "Ensures all files have proper AI Assist Notes and Debugging sections for alignment."
    category: str = "linting"

    class Arguments(BaseSkill.Arguments):
        path: str = Field(default=".", description="The directory to scan for AI context alignment.")

    async def execute(self, args: Dict[str, Any]) -> str:
        scan_path = args.get("path", ".")
        
        workspace_root = os.environ.get("WORKSPACE_ROOT", os.getcwd())
        if not os.path.isabs(scan_path):
            scan_path = os.path.join(workspace_root, scan_path)

        self.log(f"Auditing AI context in {scan_path}")
        
        # We'll import the logic from the script
        # Note: In a real production environment, we'd refactor the script to be a proper package
        script_path = Path(workspace_root) / "execution" / "verify_ai_context.py"
        
        try:
            import importlib.util
            spec = importlib.util.spec_from_file_location("verify_ai_context", str(script_path))
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            
            # The verify_ai_context script uses os.walk and prints results
            # We want to capture its results. Since we refactored SkillRegistry to capture stdout,
            # we can just call its main-like logic or a specific function.
            # However, the script doesn't return the data easily. 
            # Let's use its 'verify_file' function in a loop here for better control.
            
            from datetime import datetime
            
            results = []
            scan_root = Path(scan_path).resolve()
            
            for root, dirs, files in os.walk(scan_root):
                dirs[:] = [d for d in dirs if d not in module.SKIP_DIRS]
                for file in files:
                    if Path(file).suffix.lower() in module.EXTENSIONS:
                        res = module.verify_file(Path(root) / file)
                        results.append(res)

            passed = [r for r in results if r["passed"]]
            failed = [r for r in results if not r["passed"]]
            
            report = {
                "summary": {
                    "total": len(results),
                    "passed": len(passed),
                    "failed": len(failed),
                    "timestamp": datetime.now().isoformat()
                },
                "failures": failed[:50] # Cap for response size
            }
            
            return json.dumps(report, indent=2)

        except Exception as e:
            return f"Error during AI context audit: {str(e)}"

# Metadata: [verify_context_skill]

# Metadata: [verify_context_skill]
