"""
@docs ARCHITECTURE:Core

### AI Context Alignment
- **Subsystem**: Infrastructure Automation / execution/lib
- **Primary Entrypoints**: none declared

### ⚠️ Invariants & Non-Negotiables
- `[Structural]` Re-exports core Tadpole IPC client abstractions.

### 🔍 Debugging & Observability
# - **Telemetry Link**: Search `[__init__]` in system logs.
- **Local Errors**: none
- **Telemetry Targets**: none declared
- **Witness Tests**: none declared
"""

from .mcp_client import TadpoleClient, TadpoleClientError

__all__ = ["TadpoleClient", "TadpoleClientError"]
# Metadata: [__init__]
