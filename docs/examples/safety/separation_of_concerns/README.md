# Automotive Power Supply - Separation of Concerns Example

This example demonstrates the multi-phase safety architecture with proper separation of concerns between safety engineers and board designers.

## File Organization

```
safety/separation_of_concerns/
├── README.md                           # This file
├── phase1_safety/                      # Safety Engineer Domain
│   ├── system_safety_analysis.bhdl     # Phase 1: System-level safety
│   └── safety_requirements.bhdl        # Generated requirements
├── phase2_board/                       # Board Designer Domain  
│   ├── board_design.bhdl               # Board implementation
│   └── component_selection.md          # Design decisions
├── phase3_validation/                  # Tool-Generated Analysis
│   ├── automatic_safety_validation.bhdl # Real-time analysis results
│   ├── fmea_report.bhdl                # Generated FMEA
│   └── compliance_report.md            # ISO 26262 compliance
└── integration/
    └── system_integration.bhdl         # Combined view for tools

```

## Workflow

### Phase 1: Safety Engineer (Day 1)
1. **Creates**: `system_safety_analysis.bhdl` - Functional blocks and safety goals
2. **Generates**: `safety_requirements.bhdl` - Requirements for board designer
3. **Focus**: Hazards, safety functions, ASIL allocations

### Phase 2: Board Designer (Parallel Development) 
1. **Reads**: Generated requirements from safety analysis
2. **Creates**: `board_design.bhdl` - Circuit implementation
3. **Focus**: Component selection, routing, electrical design
4. **Freedom**: Choose any implementation meeting requirements

### Phase 3: Tool Analysis (Continuous)
1. **Monitors**: Both safety and board files
2. **Generates**: Real-time safety analysis and validation
3. **Updates**: FMEA and compliance reports automatically
4. **Alerts**: When requirements not met or gaps identified

## Key Principles

1. **Domain Expertise**: Each professional works in their area of expertise
2. **Minimal Interface**: Only essential requirements cross domain boundaries  
3. **Automatic Validation**: Tools handle compliance checking
4. **Trust-Based**: No micromanagement between domains
5. **Parallel Work**: Safety and board design proceed simultaneously

## Files in This Example

- **Safety files**: Abstract, functional, requirement-focused
- **Board files**: Concrete, electrical, implementation-focused  
- **Tool files**: Automated analysis and validation
- **Integration**: System-level view combining all domains