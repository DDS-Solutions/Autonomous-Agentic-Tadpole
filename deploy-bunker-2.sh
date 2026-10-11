#!/bin/sh
set -e
exec sh "$(dirname "$0")/scripts/deploy-bunker-2.sh" "$@"
