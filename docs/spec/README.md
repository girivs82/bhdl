# BHDL Specification Documentation

## Current Specification (v2.0)

**Primary Reference**: `BHDL_Complete_Specification.md` - This is the complete and authoritative v2.0 specification.

All specification content has been consolidated into this single comprehensive document to prevent documentation drift and maintain consistency.

## Archived Documents

Old syntax documents have been moved to `*.old` files for reference:

- `BHDL_Specification.md.old` - Original v1.0 specification
- `BHDL_Specification_Cleaned.md.old` - Cleaned v1.0 specification  
- `Bus_Interface_Specification.md.old` - Old bus interface syntax

## Parser Implementation Status

⚠️ **Important**: The current parser implementation in `bhdl-parser/` needs to be updated to support the v2.0 flow-based syntax specified in `BHDL_Complete_Specification.md`.

Currently the parser only supports the old v1.0 structured syntax (components {}, connections {}, etc.) and does not yet support:

- Flow operators (`->`, `|>`) 
- Direct component instantiation (`VCC -> Res(4.7kΩ).1 -> LED(red).A`)
- Generate constructs
- Flow specifications

## Next Steps

1. Update parser grammar to support v2.0 syntax
2. Update examples to use v2.0 syntax
3. Test end-to-end pipeline with v2.0 syntax