#!/bin/bash
# Script to organize test files into proper structure

echo "Organizing BHDL test files..."

# Move test binaries to scratch for manual sorting
echo "Moving test .rs files to tests/scratch/ for manual organization..."
mv test_*.rs demo_*.rs debug_*.rs tests/scratch/ 2>/dev/null || true

# Already moved BHDL and SVG files in previous commands

# List what needs manual intervention
echo -e "\nFiles in tests/scratch/ need manual organization:"
echo "- Integration test binaries should go to appropriate crate's src/bin/"
echo "- Utility scripts can stay in tests/integration/"

echo -e "\nRemaining files in project root:"
ls -la *.rs *.bhdl *.svg 2>/dev/null | wc -l

echo -e "\nTest organization structure:"
tree tests/ -d

echo -e "\nNext steps:"
echo "1. Move test binaries from tests/scratch/ to appropriate crate src/bin/ directories"
echo "2. Update test runners to use new paths (tests/circuits/ for BHDL files)"
echo "3. Configure output directory for generated files"
echo "4. Clean up any remaining test files from root"