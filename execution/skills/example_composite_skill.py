"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: Example Composite Skill**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
"""

from core.base_skill import BaseSkill
from pydantic import Field
from typing import Dict, Any

class GreetingSkill(BaseSkill):
    name = "greeting_skill"
    description = "A simple skill that returns a personalized greeting."
    
    class Arguments(BaseSkill.Arguments):
        name: str = Field(..., description="The name of the person to greet.")
        prefix: str = Field("Hello", description="The greeting prefix.")

    async def execute(self, args: Dict[str, Any]) -> str:
        return f"{args['prefix']}, {args['name']}! Welcome to the modular Tadpole OS."

class ResearchOrchestrator(BaseSkill):
    name = "research_orchestrator"
    description = "A composite skill that greets the user and then simulates a research task."
    category = "orchestrator"
    
    class Arguments(BaseSkill.Arguments):
        topic: str = Field(..., description="The topic to research.")
        user_name: str = Field(..., description="The name of the user requesting research.")

    async def execute(self, args: Dict[str, Any]) -> str:
        self.log(f"Starting research on {args['topic']}")
        
        # 1. Call a sub-skill (Greeting)
        greeting = await self.call_sub_skill("greeting_skill", {"name": args['user_name']})
        
        # 2. Simulate internal logic
        report = f"{greeting}\n\nResearch Report on '{args['topic']}':\n"
        report += "- Key Finding 1: Modular agents are more efficient.\n"
        report += "- Key Finding 2: Tadpole OS is evolving fast.\n"
        
        return report

# Metadata: [example_composite_skill]

# Metadata: [example_composite_skill]
