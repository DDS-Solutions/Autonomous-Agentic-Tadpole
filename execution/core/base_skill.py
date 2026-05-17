"""
@docs ARCHITECTURE:Core

### AI Assist Note
**🛡️ Tadpole OS: BaseSkill Abstract Base Class**
Provides the base blueprint for modular class-based skills in the Tadpole OS execution layer.
Ensures unified logging and runtime composition via SkillRegistry orchestration.

### 🔍 Debugging & Observability
- **Failure Path**: Registry missing for sub-skill calls, pydantic schema validation failures.
- **Telemetry Link**: Search for `[BaseSkill]` in skill execution logs.
"""

from abc import ABC, abstractmethod
from typing import Type, List, Optional, Any, Dict
from pydantic import BaseModel, ConfigDict
import json

class BaseSkill(ABC):
    """
    Base class for all Tadpole OS Skills.
    Provides automated MCP tool registration and composition support.
    """
    name: str = "base_skill"
    description: str = "Base skill description"
    version: str = "1.0.0"
    tags: List[str] = []
    
    # Metadata for filtering and routing
    category: str = "utility"
    
    class Arguments(BaseModel):
        """Override this in subclasses to define the tool schema."""
        model_config = ConfigDict(extra='allow')

    def __init__(self, registry: Any = None):
        """
        Initialize the skill.
        :param registry: The SkillRegistry instance, allowing for composition.
        """
        self.registry = registry

    @classmethod
    def get_schema(cls) -> Dict[str, Any]:
        """Returns the JSON Schema for the skill's arguments."""
        # MCP expects a specific JSON schema format (usually Draft 7)
        schema = cls.Arguments.model_json_schema()
        # Clean up Pydantic-specific fields if necessary
        if "title" in schema:
            del schema["title"]
        return schema

    @abstractmethod
    async def execute(self, args: Dict[str, Any]) -> str:
        """
        Core execution logic for the skill.
        :param args: Validated arguments as a dictionary.
        :return: Result string to be sent back to the agent.
        """
        pass
        
    def log(self, message: str):
        """Standardized logging for skills."""
        print(f"[{self.name}] [BaseSkill] {message}")

    async def call_sub_skill(self, name: str, arguments: Dict[str, Any]) -> str:
        """
        Helper to call another skill from within this skill (Composition).
        """
        if not self.registry:
            return f"Error: Skill {self.name} has no registry attached and cannot call sub-skills."
        
        return await self.registry.call_skill(name, arguments)

class CompositeSkill(BaseSkill):
    """
    A specialized skill designed for orchestrating other skills.
    """
    category: str = "orchestrator"
    
    @abstractmethod
    async def execute(self, args: Dict[str, Any]) -> str:
        """Composite skills usually coordinate multiple sub-calls here."""
        pass

# Metadata: [base_skill]

# Metadata: [base_skill]
