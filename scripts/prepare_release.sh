#!/bin/bash
# Script to help prepare BHDL releases

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Get version from argument
VERSION=$1
if [ -z "$VERSION" ]; then
    echo -e "${RED}Error: Version number required${NC}"
    echo "Usage: $0 <version>"
    echo "Example: $0 0.1.0"
    exit 1
fi

echo -e "${GREEN}Preparing BHDL release v${VERSION}${NC}"
echo "======================================"

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo -e "${YELLOW}Warning: Not on main branch (current: $CURRENT_BRANCH)${NC}"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo -e "${RED}Error: Uncommitted changes detected${NC}"
    echo "Please commit or stash changes before releasing"
    exit 1
fi

echo "Running pre-release checks..."
echo "----------------------------"

# Run tests
echo -n "Running tests... "
if cargo test --all --quiet 2>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}Tests failed! Fix before releasing.${NC}"
    exit 1
fi

# Run clippy
echo -n "Running clippy... "
if cargo clippy --all -- -D warnings 2>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}Clippy warnings found! Fix before releasing.${NC}"
    exit 1
fi

# Check formatting
echo -n "Checking formatting... "
if cargo fmt --all -- --check 2>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}Code needs formatting! Run 'cargo fmt --all'${NC}"
    exit 1
fi

# Check documentation
echo -n "Building documentation... "
if cargo doc --no-deps --all-features --quiet 2>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}Documentation build failed!${NC}"
    exit 1
fi

# Security audit
echo -n "Running security audit... "
if cargo audit 2>/dev/null; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${YELLOW}⚠${NC} (security issues found, review carefully)"
fi

echo
echo "Version Update Tasks:"
echo "--------------------"
echo "Please manually update:"
echo "1. Version in all Cargo.toml files"
echo "2. CHANGELOG.md with release notes"
echo "3. README.md version references"
echo "4. Defensive publication dates"
echo
echo "After updating, run:"
echo "  git add -A"
echo "  git commit -m \"chore: prepare release v${VERSION}\""
echo "  git tag -a v${VERSION} -m \"Release version ${VERSION}\""
echo "  git push origin main"
echo "  git push origin v${VERSION}"
echo
echo -e "${GREEN}Pre-release checks complete!${NC}"