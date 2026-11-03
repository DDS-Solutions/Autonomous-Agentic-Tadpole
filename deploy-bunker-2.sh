#!/bin/sh
# @docs ARCHITECTURE:Core
#
# ### AI Assist Note
# **Core technical resource for the Tadpole OS Sovereign infrastructure.**
# Shell-level automation for the Tadpole OS deployment and maintenance.
#
# ### 🔍 Debugging & Observability
# - **Failure Path**: Environment error, permission denied, or command failure.
//! - **Telemetry Link**: Search `[deploy_bunker_2]` in system logs.

set -e
exec sh "$(dirname "$0")/scripts/deploy-bunker-2.sh" "$@"
# Metadata: [deploy_bunker_2]
