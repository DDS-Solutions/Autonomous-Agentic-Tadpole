#!/usr/bin/env python3
"""
@docs ARCHITECTURE:Security
@docs OPERATIONS_MANUAL:Security

### AI Assist Note
**Token Rotator**: Performs zero-downtime token rotation for the NEURAL_TOKEN
administrative key. Automatically creates a dual-token configuration window
by defining NEURAL_TOKEN_OLD and NEURAL_TOKEN_NEW in the .env file.
Once clients are migrated, running with `--confirm` revokes the old tokens.

### 🔍 Debugging & Observability
- **Failure Path**: Missing or unreadable `.env` file, write permission errors,
  or premature confirmation before clients have refreshed their keys.
- **Telemetry Link**: Search `[rotate_token]` in system logs.
"""

import os
import sys
import io
import secrets
import time
import argparse
from pathlib import Path

# Ensure stdout handles UTF-8 on Windows
if sys.platform == "win32":
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding='utf-8', errors='replace')

ENV_FILE = Path(".env")

def load_env_lines():
    if not ENV_FILE.exists():
        # Try finding in parent directory if run from execution
        candidate = Path("../.env")
        if candidate.exists():
            return candidate, candidate.read_text().splitlines()
        # Create empty .env if not found
        return ENV_FILE, []
    return ENV_FILE, ENV_FILE.read_text().splitlines()

def rotate_token(grace_period_secs=300):
    new_token = secrets.token_hex(32)
    path, lines = load_env_lines()
    
    old_token = None
    new_lines = []
    token_found = False
    
    for line in lines:
        line_strip = line.strip()
        # Find active NEURAL_TOKEN
        if line_strip.startswith("NEURAL_TOKEN=") and not line_strip.startswith("NEURAL_TOKEN_NEW=") and not line_strip.startswith("NEURAL_TOKEN_OLD="):
            old_token = line_strip.split("=", 1)[1].strip('"').strip("'")
            new_lines.append(f"NEURAL_TOKEN_OLD={old_token}")
            new_lines.append(f"NEURAL_TOKEN={new_token}")
            token_found = True
        elif line_strip.startswith("NEURAL_TOKEN_OLD=") or line_strip.startswith("NEURAL_TOKEN_NEW="):
            # Skip any existing rotation values from previous attempts
            continue
        else:
            new_lines.append(line)
            
    if not token_found:
        # If no active token found, write it new
        new_lines.append(f"NEURAL_TOKEN={new_token}")
        
    # Append NEURAL_TOKEN_NEW helper
    new_lines.append(f"NEURAL_TOKEN_NEW={new_token}")
    
    path.write_text("\n".join(new_lines) + "\n")
    
    print("✅ Zero-downtime token rotation initiated.")
    print(f"   New token: {new_token}")
    print(f"   Old token kept valid as NEURAL_TOKEN_OLD.")
    print(f"   Grace period active. Please update your client configurations.")
    print(f"   Ensure you confirm this rotation after clients migrate by running:")
    print(f"   python execution/rotate_token.py --confirm")

def confirm_rotation():
    path, lines = load_env_lines()
    new_lines = []
    confirmed = False
    
    for line in lines:
        line_strip = line.strip()
        if line_strip.startswith("NEURAL_TOKEN_OLD=") or line_strip.startswith("NEURAL_TOKEN_NEW="):
            confirmed = True
            continue
        else:
            new_lines.append(line)
            
    if not confirmed:
        print("ℹ️ No active rotation grace period found. Token is already in singular state.")
        return

    path.write_text("\n".join(new_lines) + "\n")
    print("✅ Token rotation confirmed. NEURAL_TOKEN_OLD and NEURAL_TOKEN_NEW removed.")
    print("   The grace period has ended; only the active NEURAL_TOKEN is valid.")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Zero-Downtime Neural Token Rotation Runbook")
    parser.add_argument("--confirm", action="store_true", help="Confirm rotation, revoking old grace tokens")
    parser.add_argument("--grace-secs", type=int, default=300, help="Grace period duration in seconds (default: 300)")
    args = parser.parse_args()
    
    if args.confirm:
        confirm_rotation()
    else:
        rotate_token(args.grace_secs)

# Metadata: [rotate_token]
