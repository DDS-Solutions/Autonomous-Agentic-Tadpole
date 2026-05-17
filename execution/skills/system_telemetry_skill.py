"""
@docs ARCHITECTURE:Infrastructure

### AI Assist Note
**🛡️ Tadpole OS: System Telemetry Skill**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
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
                # Use WMIC for Windows CPU load
                cmd = "wmic cpu get loadpercentage"
                output = subprocess.check_output(cmd, shell=True).decode()
                lines = [l.strip() for l in output.split('\n') if l.strip()]
                if len(lines) > 1:
                    return f"{lines[1]}%"
            else:
                # Use loadavg for Unix
                load1, load5, load15 = os.getloadavg()
                return f"{load1 * 100 / os.cpu_count():.1f}% (1min avg)"
        except Exception:
            return "Unknown"
        return "Unknown"

    def _get_memory_info(self) -> Dict[str, str]:
        try:
            if platform.system() == "Windows":
                # Use WMIC for Windows memory
                cmd = "wmic OS get FreePhysicalMemory,TotalVisibleMemorySize /Value"
                output = subprocess.check_output(cmd, shell=True).decode()
                data = {}
                for line in output.split('\n'):
                    if '=' in line:
                        k, v = line.split('=')
                        data[k.strip()] = int(v.strip())
                
                total = data.get("TotalVisibleMemorySize", 0) * 1024 # KB to Bytes
                free = data.get("FreePhysicalMemory", 0) * 1024
                used = total - free
                return {
                    "total": f"{total / (1024**3):.2f} GB",
                    "used": f"{used / (1024**3):.2f} GB",
                    "free": f"{free / (1024**3):.2f} GB",
                    "usage_pct": f"{(used/total)*100:.1f}%"
                }
            else:
                # Fallback or /proc/meminfo
                return {"status": "Manual retrieval required for non-Windows"}
        except Exception:
            return {"error": "Failed to retrieve memory info"}

    def _get_storage_info(self, path_str: str) -> Dict[str, Any]:
        path = Path(path_str).absolute()
        try:
            usage = shutil.disk_usage(path)
            info = {
                "path": str(path),
                "total": f"{usage.total / (1024**3):.2f} GB",
                "used": f"{usage.used / (1024**3):.2f} GB",
                "free": f"{usage.free / (1024**3):.2f} GB",
                "usage_pct": f"{(usage.used / usage.total) * 100:.1f}%"
            }
            if path.is_file():
                info["file_size_bytes"] = path.stat().st_size
                info["last_modified"] = time.ctime(path.stat().st_mtime)
            return info
        except Exception as e:
            return {"error": str(e)}

    def _get_process_snapshot(self) -> List[str]:
        try:
            if platform.system() == "Windows":
                cmd = "tasklist /FI \"STATUS eq running\" /FO CSV /NH"
                output = subprocess.check_output(cmd, shell=True).decode()
                return output.splitlines()[:10] # Top 10
            return ["Snapshot only available on Windows"]
        except Exception:
            return ["Error retrieving snapshot"]

    def _format_output(self, data: Dict[str, Any]) -> str:
        import json
        return json.dumps(data, indent=2)

# Metadata: [system_telemetry_skill]

# Metadata: [system_telemetry_skill]
