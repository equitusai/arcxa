#!/usr/bin/env bash
set -euo pipefail

DB2_PROFILE="/database/config/db2inst1/sqllib/db2profile"

if [ -f "$DB2_PROFILE" ]; then
  # shellcheck disable=SC1090
  . "$DB2_PROFILE"
fi

exec /app/arcxa-coordinator "$@"
