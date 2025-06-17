# BHDL Testing Guide

## Overview

All test files and outputs are now organized in the `tests/` directory to keep the project root clean.

## Test Organization

### Directory Structure
```
tests/
├── circuits/          # Test circuit files (.bhdl)
│   ├── simple/       # Basic test circuits
│   ├── realistic/    # Real-world circuits (7805, etc.)
│   └── edge_cases/   # Edge case testing
├── outputs/          # Test outputs
│   ├── svg/         # Generated visualizations
│   └── netlists/    # Generated netlists
├── integration/      # Integration test utilities
├── scratch/         # Temporary files (git-ignored)
├── test_config.rs   # Shared test configuration
├── run_tests.sh     # Test runner script
└── README.md        # Test documentation
```

### Running Tests

#### Quick Test Commands
```bash
# Run all tests
./tests/run_tests.sh all

# Run specific test suite
./tests/run_tests.sh analyzer
./tests/run_tests.sh synthesizer
./tests/run_tests.sh visualizer
./tests/run_tests.sh parser
./tests/run_tests.sh e2e

# Run specific test binary
cargo run -p bhdl-synthesizer --bin test_7805_realistic
```

#### Unit Tests
```bash
cargo test                    # All unit tests
cargo test -p bhdl-analyzer  # Specific crate
```

## Best Practices

### 1. Test Binary Location
Test binaries belong in `<crate>/src/bin/`:
- `bhdl-parser/src/bin/` - Parser tests
- `bhdl-analyzer/src/bin/` - Analyzer tests
- `bhdl-synthesizer/src/bin/` - Synthesizer tests
- `bhdl-visualizer/src/bin/` - Visualizer tests

### 2. Test Circuits
Place test circuits in `tests/circuits/`:
- `simple/` - Basic functionality tests
- `realistic/` - Real-world examples
- `edge_cases/` - Error handling and edge cases

### 3. Test Outputs
Configure tests to write outputs to `tests/outputs/`:
- SVG visualizations → `tests/outputs/svg/`
- Netlists → `tests/outputs/netlists/`
- Logs → `tests/outputs/logs/`

### 4. Using Test Configuration
Include the test configuration in your test binaries:

```rust
// In test binary
#[path = "../../../tests/test_config.rs"]
mod test_config;

use test_config::*;

fn main() {
    // Get test circuit path
    let circuit = test_config::circuits::test_7805();
    
    // Get output path
    let output = test_config::outputs::svg("my_test");
    
    // Your test logic here
}
```

### 5. Command Line Arguments
Test binaries should accept circuit paths as arguments:

```rust
let test_file = std::env::args().nth(1)
    .unwrap_or_else(|| "tests/circuits/realistic/test.bhdl".to_string());
```

## Migration from Root Directory

The project root previously contained 80+ test files. These have been organized:
- Test binaries (.rs) → Moved to crate `src/bin/` directories
- Test circuits (.bhdl) → Moved to `tests/circuits/`
- Output files (.svg) → Moved to `tests/outputs/svg/`
- Temporary files → Moved to `tests/scratch/` (git-ignored)

## Benefits

1. **Clean project root** - No clutter from test files
2. **Organized structure** - Easy to find test files
3. **Git-friendly** - Outputs in ignored directories
4. **Consistent paths** - Tests know where to find/write files
5. **Better CI/CD** - Clear test organization for automation

## TODO

- [ ] Update all test binaries to use test_config paths
- [ ] Configure visualizer to output to tests/outputs/svg/
- [ ] Add test result summary generation
- [ ] Create test coverage reports
- [ ] Add performance benchmarks