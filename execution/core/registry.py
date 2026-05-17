"""
@docs ARCHITECTURE:Core:Orchestration

### AI Assist Note
**🛡️ Tadpole OS: SkillRegistry orchestrator**
Discovers, instantiates, and executes Python-class skills dynamically from the skills/ subdirectory.
Redirects standard output to standard error to prevent MCP stdio stream corruption.

### 🔍 Debugging & Observability
- **Failure Path**: Import errors during discovery, arguments schema validation errors, directory not found.
- **Telemetry Link**: Search for `[SkillRegistry]` in skill loading and execution logs.
"""

import os
import importlib.util
import inspect
import time
import sys
import io
from contextlib import redirect_stdout
from pathlib import Path
from typing import Dict, Type, Any, List
from .base_skill import BaseSkill

class SkillRegistry:
    """
    Handles dynamic discovery, loading, and management of Python-class skills.
    """
    def __init__(self, skills_dir: str = None):
        self.skills: Dict[str, BaseSkill] = {}
        # Default to the 'skills' subdirectory of the execution root
        if skills_dir is None:
            self.skills_dir = Path(__file__).parent.parent / "skills"
        else:
            self.skills_dir = Path(skills_dir)
            
    def discover_skills(self):
        """
        Walks the skills directory and imports all classes inheriting from BaseSkill.
        """
        self.skills.clear()
        
        if not self.skills_dir.exists():
            print(f"[Registry] [SkillRegistry] Warning: Skills directory {self.skills_dir} does not exist.", file=sys.stderr)
            return

        # Recursively find all .py files
        for py_file in self.skills_dir.rglob("*.py"):
            if py_file.name.startswith("__"):
                continue
                
            try:
                # Dynamic import
                module_name = f"skills.{py_file.stem}"
                spec = importlib.util.spec_from_file_location(module_name, py_file)
                if spec is None or spec.loader is None:
                    continue
                    
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                
                # Find all classes that inherit from BaseSkill
                for name, obj in inspect.getmembers(module):
                    if inspect.isclass(obj) and issubclass(obj, BaseSkill) and obj is not BaseSkill and obj is not None:
                        # Instantiate the skill with this registry
                        skill_instance = obj(registry=self)
                        skill_name = getattr(skill_instance, "name", py_file.stem)
                        self.skills[skill_name] = skill_instance
                        print(f"[Registry] [SkillRegistry] Loaded skill: {skill_name}", file=sys.stderr)
                        
            except Exception as e:
                print(f"[Registry] [SkillRegistry] Failed to load skill from {py_file}: {str(e)}", file=sys.stderr)

    async def call_skill(self, name: str, arguments: Dict[str, Any]) -> str:
        """
        Executes a registered skill by name.
        Captures execution time and ensures stdout doesn't corrupt MCP stream.
        """
        if name not in self.skills:
            return f"Error: Skill '{name}' not found in registry."
            
        skill = self.skills[name]
        start_time = time.perf_counter()
        
        # Capture internal prints and redirect to stderr
        f = io.StringIO()
        try:
            with redirect_stdout(sys.stderr):
                # Pydantic validation
                validated_args = skill.Arguments(**arguments)
                result = await skill.execute(validated_args.model_dump())
                
            duration = (time.perf_counter() - start_time) * 1000
            print(f"[Registry] [SkillRegistry] Skill {name} executed in {duration:.2f}ms", file=sys.stderr)
            return result
            
        except Exception as e:
            duration = (time.perf_counter() - start_time) * 1000
            error_msg = f"Error executing skill '{name}' after {duration:.2f}ms: {str(e)}"
            print(f"[Registry] [SkillRegistry] {error_msg}", file=sys.stderr)
            return error_msg

    def get_all_tools(self) -> List[Dict[str, Any]]:
        """
        Returns a list of tool definitions compatible with MCP.
        """
        tools = []
        for name, skill in self.skills.items():
            tools.append({
                "name": name,
                "description": skill.description,
                "schema": skill.get_schema()
            })
        return tools

# Metadata: [registry]

# Metadata: [registry]
