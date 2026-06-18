#!/bin/bash
# generate-docs.sh - Automation script for generating SVG dependency diagrams
# and AI-optimized Markdown architecture documentation
#
# This script:
# 1. Checks if cargo-arc is installed, installs if not
# 2. Runs `cargo arc -o deps.svg` to generate SVG dependency diagrams
# 3. Generates ARCHITECTURE_DIAGRAMS.md with AI-optimized Mermaid diagrams
# 4. Generates README.md with workspace information
# 5. Generates individual crate markdown files in output-md/
#
# Usage: ./generate-docs.sh [workspace-path]
#   workspace-path: Path to the Cargo workspace (default: current directory)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default workspace path
WORKSPACE_PATH="${1:-.}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HELPER_BINARY="${SCRIPT_DIR}/generate-docs-helper/target/release/cargo-arc-md"

echo -e "${GREEN}=== Architecture Documentation Generator ===${NC}"
echo -e "${YELLOW}Workspace path: ${WORKSPACE_PATH}${NC}"

# Check if cargo-arc is installed
if ! command -v cargo arc &> /dev/null; then
    echo -e "${YELLOW}cargo-arc not found, installing...${NC}"
    cargo install cargo-arc
    echo -e "${GREEN}✓ cargo-arc installed${NC}"
else
    echo -e "${GREEN}✓ cargo-arc found${NC}"
fi

# Build the helper binary if needed
if [ ! -f "${HELPER_BINARY}" ]; then
    echo -e "${YELLOW}Building cargo-arc-md helper...${NC}"
    cd "${SCRIPT_DIR}/generate-docs-helper"
    cargo build --release
    cd "${SCRIPT_DIR}"
    echo -e "${GREEN}✓ Helper built${NC}"
fi

# Run the helper to generate documentation
echo -e "${YELLOW}Generating documentation...${NC}"
"${HELPER_BINARY}" "${WORKSPACE_PATH}"

echo -e "${GREEN}=== Documentation Generation Complete ===${NC}"
echo ""
echo "Generated files:"
echo "  - ${WORKSPACE_PATH}/deps.svg"
echo "  - ${WORKSPACE_PATH}/ARCHITECTURE_DIAGRAMS.md"
echo "  - ${WORKSPACE_PATH}/README.md"
echo "  - ${WORKSPACE_PATH}/output-md/ (individual crate documentation)"
echo ""
