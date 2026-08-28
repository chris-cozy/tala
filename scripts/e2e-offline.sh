#!/bin/sh
set -eu
TALA_REPOSITORY="$(CDPATH= cd "$(dirname "$0")/.." && pwd)"
exec /usr/bin/sandbox-exec -f "$TALA_REPOSITORY/scripts/offline.sb" "$TALA_REPOSITORY/artifacts/e2e/bin/tala" "$@"
