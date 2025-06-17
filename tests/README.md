# BHDL Test Organization

This directory contains all test files and outputs for the BHDL project.

## Directory Structure

```
tests/
├── integration/        # Integration test binaries (.rs files)
│   └── *.rs           # Test binaries that should be in crate src/bin/
├── circuits/          # Test circuit files (.bhdl)
│   ├── simple/        # Simple test circuits
│   ├── realistic/     # Realistic circuits (7805, etc.)
│   └── edge_cases/    # Edge case testing
├── outputs/           # Test outputs (SVG, logs, etc.)
│   ├── svg/           # Generated circuit visualizations
│   └── netlists/      # Generated netlist outputs
└── scratch/           # Temporary test files (git-ignored)
```

## Running Tests

### Unit Tests
```bash
# Run all unit tests
cargo test

# Run tests for specific crate
cargo test -p bhdl-analyzer
cargo test -p bhdl-synthesizer
```

### Integration Tests
Integration test binaries should be placed in the appropriate crate's `src/bin/` directory:
- `bhdl-analyzer/src/bin/` - Analyzer test binaries
- `bhdl-synthesizer/src/bin/` - Synthesizer test binaries
- `bhdl-visualizer/src/bin/` - Visualizer test binaries

Run them with:
```bash
cargo run -p <crate-name> --bin <test-name>
```

### Test Circuits
Test circuits should be organized in `tests/circuits/`:
- Use descriptive names
- Group by complexity or feature being tested
- Document what each circuit tests

## Guidelines

1. **No test files in project root** - All test files belong here
2. **Clean up after tests** - Don't leave temporary files around
3. **Use scratch/ for experiments** - This directory is git-ignored
4. **Organize by purpose** - Keep similar tests together
5. **Document test purpose** - Each test should explain what it's testing

## Migration TODO
- [ ] Move all *.rs test files from root to appropriate crate bins
- [ ] Move all test *.bhdl files to tests/circuits/
- [ ] Move all *.svg outputs to tests/outputs/svg/
- [ ] Update test runners to use new paths
- [ ] Add tests/scratch/ to .gitignore