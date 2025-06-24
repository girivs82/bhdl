# Changelog Entry

## [Unreleased] - 2025-06-24

### Added
- **Full BHDL Expression Evaluator**: New expression evaluation system in `bhdl-common` that supports:
  - Module parsing and constant extraction
  - Struct literal evaluation
  - Path expressions (e.g., `params.output_voltage`)
  - Electrical unit conversion (kΩ, mA, µF, etc.)
  - Runtime parameter resolution

- **v2.0 Flow Syntax Test Suite**: Comprehensive test coverage for flow syntax validation

- **Runtime Model Execution**: Integration of expression evaluator with SPICE models for dynamic parameter evaluation

### Fixed
- **Component Library Loading**: Component types in v2.0 flow syntax (e.g., `Res(10k).1`) are now correctly extracted instead of showing as "Unknown"

- **Parameter Validation**: String parameters like colors (`red`, `green`, `blue`) and package types are no longer incorrectly flagged as undefined constants

- **Flow Syntax Validation**: Instance names created inline in flow syntax (e.g., `R1: Res(10k)`) are no longer flagged as undefined symbols

### Changed
- **Analyzer Pass 2**: Enhanced to handle FLOW_EXPR nodes and recognize component instantiations within flow expressions

- **Analyzer Pass 3**: Improved to distinguish between string parameter values and constant references

- **Parser**: Added support for `where` and `with` keywords for connection constraints

### Technical Details
- Component type extraction now detects CONNECTION_STMT context for v2.0 inline syntax
- IDENT_REF validation skips instance creation patterns in flow/connection contexts
- Parameter evaluation includes a whitelist of common string values
- Expression evaluator supports two-pass constant resolution for complex dependencies