# Changelog

All notable changes to BHDL will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Complete BHDL v2.0 parser with flow-based syntax support
- 8-pass semantic analyzer with integrated SPICE analysis
- Newton-Raphson nonlinear DC solver for accurate circuit simulation
- Component role detection based on electrical behavior
- Safety analysis system with derating and damage estimation
- Power converter stability analysis with AC integration
- Pin metadata system for accurate component classification
- Unified data model eliminating lossy conversions
- Component database with KiCad symbol integration
- Netlist synthesis with automatic reference designators
- Intent-based design capture using `for` keyword
- Hierarchical intent propagation through modules
- Flow-based power management with capacity tracking
- Comprehensive defensive publications for innovations
- Apache 2.0 licensing with patent protection

### Changed
- Migrated from v1.0 block-based to v2.0 flow-based syntax
- Removed all v1.0 syntax support
- Standardized on flow operators: `→`, `←→`, `|>`
- Component parameters moved to bhdl-stdlib

### Fixed
- Component inference using actual electrical parameters
- Topology-based role detection without naming conventions
- Power domain analysis through current flow
- Circular dependency issues in crate structure

### Security
- Added comprehensive input validation in parser
- Memory safety improvements in SPICE solver
- Path traversal prevention in file operations

## [0.1.0] - TBD

Initial public release.

### Added
- Core language implementation
- Basic CLI functionality
- Documentation and examples
- Test infrastructure

### Known Issues
- Visualizer component scaling needs improvement
- CLI commands are basic placeholders
- LSP implementation not started

## Future Releases

### [0.2.0] - Planned
- Behavioral module support
- Process blocks and state machines
- Enhanced CLI with project management
- Web playground for online experimentation

### [0.3.0] - Planned
- Language Server Protocol (LSP) implementation
- VS Code extension
- Package manager for component libraries
- Enhanced KiCad integration

### [1.0.0] - Target
- Stable language specification
- Production-ready toolchain
- Comprehensive component library
- Full documentation and tutorials