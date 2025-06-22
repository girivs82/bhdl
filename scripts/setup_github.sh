#!/bin/bash
# Script to set up GitHub repository settings

echo "GitHub Repository Setup Checklist"
echo "================================="
echo
echo "After creating the repository on GitHub, configure these settings:"
echo
echo "1. Repository Settings:"
echo "   - Description: 'Board Hardware Description Language - Describing circuits the way engineers think'"
echo "   - Website: https://github.com/[USERNAME]/bhdl"
echo "   - Topics: rust, hdl, hardware-description-language, eda, pcb-design, circuit-simulation, spice, electronic-design"
echo
echo "2. Default Branch:"
echo "   - Ensure 'main' is the default branch"
echo
echo "3. Branch Protection (Settings > Branches):"
echo "   - Protect 'main' branch"
echo "   - Require pull request reviews (1)"
echo "   - Require status checks (CI)"
echo "   - Require branches to be up to date"
echo
echo "4. Security (Settings > Security):"
echo "   - Enable Dependabot alerts"
echo "   - Enable Dependabot security updates"
echo "   - Enable secret scanning"
echo
echo "5. Features to Enable:"
echo "   - Issues"
echo "   - Discussions"
echo "   - Projects (for roadmap)"
echo "   - Wiki (optional)"
echo
echo "6. Pages (for documentation):"
echo "   - Source: Deploy from branch"
echo "   - Branch: main"
echo "   - Folder: /docs"
echo
echo "7. Add Repository Secrets (for CI):"
echo "   - CODECOV_TOKEN (if using codecov)"
echo
echo "8. First Push Commands:"
echo "   git remote add origin https://github.com/[USERNAME]/bhdl.git"
echo "   git branch -M main"
echo "   git push -u origin main"
echo
echo "9. After Push:"
echo "   - Check Actions tab to ensure CI runs"
echo "   - Create initial GitHub Release (v0.0.1-pre)"
echo "   - Pin important issues/discussions"
echo "   - Add README badges with correct URLs"
echo
echo "10. Community Setup:"
echo "    - Create 'good first issue' labels"
echo "    - Create 'help wanted' labels"
echo "    - Add issue templates are already created"
echo "    - Welcome new contributors warmly"