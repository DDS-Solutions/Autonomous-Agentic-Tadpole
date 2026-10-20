"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**reorder_models**: Utilities for reordering and elevating local model configurations.

### 🔍 Debugging & Observability
- **Failure Path**: Missing data/infra_models.json or invalid JSON formatting.
- **Telemetry Link**: Search `[reorder_models]` in logs.
"""

import json
from pathlib import Path

def reorder_models():
    data_path = Path("data/infra_models.json")
    with open(data_path, "r", encoding="utf-8") as f:
        models = json.load(f)

    by_id = {m["id"]: m for m in models}

    # Elevate flagship local models to agentic recommended with structured output
    if "gemma4:26b" in by_id:
        by_id["gemma4:26b"]["capabilities"]["isAgenticRecommended"] = True
        by_id["gemma4:26b"]["capabilities"]["supportsStructuredOutput"] = True
        by_id["gemma4:26b"]["capabilities"]["tags"] = ["local", "flagship", "open-weights"]

    if "gemma4:e4b" in by_id:
        by_id["gemma4:e4b"]["capabilities"]["isAgenticRecommended"] = True
        by_id["gemma4:e4b"]["capabilities"]["supportsStructuredOutput"] = True
        by_id["gemma4:e4b"]["capabilities"]["tags"] = ["local", "efficient", "open-weights"]

    if "qwen3.5:9b" in by_id:
        by_id["qwen3.5:9b"]["capabilities"]["tags"] = ["local", "qwen"]

    # Local models prioritized first, ranked by capability requirements
    local_order = [
        "gemma4:26b",
        "gemma4:e4b",
        "c2e5757e-ceba-4d8f-b8f4-9c2f12c724c0", # gemma4:12b
        "qwen3.5:9b",
        "88ed1ca0-850b-4b30-bc0d-b50fa4dbd8ea", # ornith:9b
        "f1a3fd5b-9d27-4da1-91d4-40d902c86dc5", # gemma4:e2b
        "phi3.5:latest"                          # fallback (no tools)
    ]

    # Cloud models prioritized second, ranked by capability requirements
    cloud_order = [
        "claude-3-7-sonnet",
        "gemini-2.5-pro",
        "deepseek-reasoner",
        "gpt-4o",
        "deepseek-v3",
        "qwen-max",
        "gpt-4o-mini",
        "grok-beta",
        "gemini-2.5-flash"
    ]

    all_ids = local_order + cloud_order
    assert len(all_ids) == len(models), f"Count mismatch: {len(all_ids)} vs {len(models)}"
    
    ordered_models = [by_id[mid] for mid in all_ids]

    with open(data_path, "w", encoding="utf-8") as f:
        json.dump(ordered_models, f, indent=2)
        f.write("\n")

    print(f"[reorder_models] Successfully reordered {len(ordered_models)} models: Local first, then Cloud.")

if __name__ == "__main__":
    reorder_models()
