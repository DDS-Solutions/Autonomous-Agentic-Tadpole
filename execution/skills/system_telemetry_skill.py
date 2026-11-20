"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: System Telemetry Skill**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Search `[system_telemetry_skill]` in system logs.
"""

from typing import Dict, Any, List
import os
import platform
import shutil
import subprocess
import time
from pathlib import Path
from pydantic import Field
from core.base_skill import BaseSkill

class SystemTelemetrySkill(BaseSkill):
    """
    Provides real-time hardware telemetry and file system metrics for the host machine.
    Essential for grounding agent reasoning in physical reality.
    """
    name: str = "system_telemetry"
    description: str = "Provides real-time hardware telemetry (CPU, RAM, Disk) and file system metrics for the host machine."
    version: str = "1.0.0"
    category: str = "utility"

    class Arguments(BaseSkill.Arguments):
        target_path: str = Field(default=".", description="Optional path to query for disk/file size metrics.")
        include_process_list: bool = Field(default=False, description="Whether to include a snapshot of running processes.")

    async def execute(self, args: Dict[str, Any]) -> str:
        target_path = args.get("target_path", ".")
        include_process_list = args.get("include_process_list", False)
        
        telemetry = {
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
            "os": {
                "system": platform.system(),
                "node": platform.node(),
                "release": platform.release(),
                "version": platform.version(),
                "machine": platform.machine()
            },
            "cpu": {
                "cores": os.cpu_count(),
                "usage_pct": self._get_cpu_usage()
            },
            "memory": self._get_memory_info(),
            "storage": self._get_storage_info(target_path),
            "process_id": os.getpid()
        }

        if include_process_list:
            telemetry["processes"] = self._get_process_snapshot()

        return f"### SYSTEM TELEMETRY REPORT\n{self._format_output(telemetry)}"

    def _get_cpu_usage(self) -> str:
        try:
            if platform.system() == "Windows":
                ps = shutil.which("powershell.exe") or shutil.which("pwsh")
                if ps:
                    output = subprocess.check_output(
                        [
                            ps,
                            "-NoProfile",
                            "-NonInteractive",
                            "-Command",
                            "(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average",
                        ],
                        shell=False,
                        text=True,
                        timeout=10,
                    ).strip()
                    if output:
                        return f"{output}%"
            else:
                # Use loadavg for Unix
                load1, load5, load15 = os.getloadavg()
                return f"{load1 * 100 / (os.cpu_count() or 1):.1f}% (1min avg)"
        except Exception:
            return "Unknown"
        return "Unknown"

    def _get_memory_info(self) -> Dict[str, str]:
        try:
            if platform.system() == "Windows":
                ps = shutil.which("powershell.exe") or shutil.which("pwsh")
                if not ps:
                    return {"error": "PowerShell unavailable for CIM memory query"}
                output = subprocess.check_output(
                    [
                        ps,
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        "$o=Get-CimInstance Win32_OperatingSystem; \"$($o.TotalVisibleMemorySize),$($o.FreePhysicalMemory)\"",
                    ],
                    shell=False,
                    text=True,
                    timeout=10,
                ).strip()
                parts = [p.strip() for p in output.split(",") if p.strip()]
                if len(parts) == 2:
                    total = int(parts[0]) * 1024  # KB to Bytes
                    free = int(parts[1]) * 1024
                    used = max(0, total - free)
                    pct = (used / total) * 100 if total > 0 else 0.0
                    return {
                        "total": f"{total / (1024**3):.2f} GB",
                        "used": f"{used / (1024**3):.2f} GB",
                        "free": f"{free / (1024**3):.2f} GB",
                        "usage_pct": f"{pct:.1f}%",
                    }
                return {"error": "Unexpected CIM memory output"}
            else:
                # Fallback or /proc/meminfo
                return {"status": "Manual retrieval required for non-Windows"}
        except Exception:
            return {"error": "Failed to retrieve memory info"}

    def _get_storage_info(self, path_str: str) -> Dict[str, Any]:
        workspace_root = Path(os.environ.get("WORKSPACE_ROOT", os.getcwd())).resolve()
        try:
            candidate = Path(path_str)
            resolved = (workspace_root / candidate).resolve() if not candidate.is_absolute() else candidate.resolve()
            if not resolved.is_relative_to(workspace_root):
                return {"error": "Forbidden: target_path must reside within workspace root"}
            usage = shutil.disk_usage(resolved)
            info = {
                "path": str(resolved),
                "total": f"{usage.total / (1024**3):.2f} GB",
                "used": f"{usage.used / (1024**3):.2f} GB",
                "free": f"{usage.free / (1024**3):.2f} GB",
                "usage_pct": f"{(usage.used / usage.total) * 100:.1f}%"
            }
            if resolved.is_file():
                info["file_size_bytes"] = resolved.stat().st_size
                info["last_modified"] = time.ctime(resolved.stat().st_mtime)
            return info
        except Exception as e:
            return {"error": str(e)}

    def _get_process_snapshot(self) -> List[str]:
        try:
            if platform.system() == "Windows":
                tasklist = shutil.which("tasklist.exe") or "tasklist"
                output = subprocess.check_output(
                    [tasklist, "/FI", "STATUS eq running", "/FO", "CSV", "/NH"],
                    shell=False,
                    text=True,
                    timeout=10,
                )
                return output.splitlines()[:10] # Top 10
            return ["Snapshot only available on Windows"]
        except Exception:
            return ["Error retrieving snapshot"]

    def _format_output(self, data: Dict[str, Any]) -> str:
        import json
        return json.dumps(data, indent=2)





# Metadata: [system_telemetry_skill]
