#!/bin/bash
# Validate local and ODBC-enabled build profiles in a Conda-safe environment.

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

RUST_TOOLCHAIN="stable"

run_cargo() {
    local cargo_args=("$@")
    if [ "${cargo_args[0]}" = "cargo" ]; then
        cargo_args=("${cargo_args[@]:1}")
    fi

    env -u CC \
        -u CFLAGS \
        -u CPPFLAGS \
        -u LDFLAGS \
        -u C_INCLUDE_PATH \
        -u CPLUS_INCLUDE_PATH \
        -u LIBRARY_PATH \
        PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig \
        RUSTFLAGS="" \
        cargo +${RUST_TOOLCHAIN} "${cargo_args[@]}"
}

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Validating Build Matrix${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${YELLOW}Using repo-managed Conda-safe cargo wrapper${NC}"
echo ""

echo -e "${BLUE}[1/7] arcxa-core default profile${NC}"
run_cargo cargo check -p arcxa-core --lib
echo -e "${GREEN}✓ arcxa-core default profile is clean${NC}"
echo ""

echo -e "${BLUE}[2/7] arcxa-coordinator default profile${NC}"
run_cargo cargo check -p arcxa-coordinator --lib
echo -e "${GREEN}✓ arcxa-coordinator default profile is clean${NC}"
echo ""

echo -e "${BLUE}[3/7] arcxa-core ODBC profile${NC}"
run_cargo cargo check -p arcxa-core --lib --features odbc
echo -e "${GREEN}✓ arcxa-core ODBC profile is clean${NC}"
echo ""

echo -e "${BLUE}[4/7] arcxa-coordinator ODBC profile${NC}"
run_cargo cargo check -p arcxa-coordinator --lib --features odbc
echo -e "${GREEN}✓ arcxa-coordinator ODBC profile is clean${NC}"
echo ""

echo -e "${BLUE}[5/7] shell wrapper syntax${NC}"
bash -n build.sh run-local.sh run-single-node.sh test.sh validate-build-matrix.sh
echo -e "${GREEN}✓ wrapper scripts parse cleanly${NC}"
echo ""

echo -e "${BLUE}[6/7] docker-compose parse${NC}"
python - <<'PY'
import yaml
with open('docker-compose.yml', 'r', encoding='utf-8') as f:
    yaml.safe_load(f)
print('docker-compose.yml parsed successfully')
PY
echo -e "${GREEN}✓ docker-compose.yml parses cleanly${NC}"
echo ""

echo -e "${BLUE}[7/7] git diff hygiene${NC}"
git diff --check
echo -e "${GREEN}✓ git diff --check is clean${NC}"
echo ""

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Build Matrix Validation Complete${NC}"
echo -e "${GREEN}========================================${NC}"
