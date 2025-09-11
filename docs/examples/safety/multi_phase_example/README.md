# Multi-Phase Functional Safety Example
## Automotive ECU Power Supply with ISO 26262 Compliance

This example demonstrates the complete multi-phase functional safety workflow for an automotive ECU power supply, showing how safety engineers and board designers work in parallel.

## Scenario
We're developing a power supply for an automotive Electronic Control Unit (ECU) that controls safety-critical functions. The system must meet ISO 26262 ASIL B requirements.

## Directory Structure
```
multi_phase_example/
├── README.md                           # This file
├── phase1_safety_analysis/             # Safety engineer's work (Day 1)
│   ├── system_safety_analysis.bhdl     # Hazard analysis & safety goals
│   ├── safety_requirements.bhdl        # Generated requirements templates
│   └── completed_requirements.bhdl     # Safety engineer fills templates
├── phase2_board_design/                # Board designer's work (Parallel)
│   ├── power_supply_board.bhdl         # Actual implementation
│   ├── component_selection.md          # Designer's choices & rationale
│   └── board_validation.bhdl          # Designer's satisfaction claims
└── phase3_validation/                  # Tool-generated validation
    ├── validation_report.bhdl          # Automatic compliance checking
    ├── gap_analysis.bhdl               # Missing requirements
    ├── fmea_report.bhdl                # Generated FMEA
    └── safety_metrics.bhdl             # SPFM, LFM, PMHF calculations
```

## Timeline

### Day 1 - Project Kickoff
- **Safety Engineer**: Starts Phase 1 immediately
- **Board Designer**: Starts Phase 2 in parallel
- **No waiting or dependencies**

### Week 1
- **Safety Engineer**: Completes hazard analysis, defines safety goals
- **Board Designer**: Selects architecture, begins component selection
- **Tool**: Provides real-time feedback on both

### Week 2
- **Safety Engineer**: Refines requirements based on system analysis
- **Board Designer**: Implements circuit, adds safety mechanisms
- **Tool**: Validates compliance continuously

### Week 3
- **Both**: Review validation reports, address gaps
- **Tool**: Generates final compliance documentation

## Key Principles Demonstrated

1. **Parallel Development**: Safety and design work proceed simultaneously
2. **Clear Separation**: Each role has distinct responsibilities
3. **No Over-Specification**: Safety requirements are functional, not prescriptive
4. **Tool Validation**: Automatic checking of satisfaction claims
5. **Context-Sensitive**: Effects generated based on actual implementation

## How to Use This Example

1. **Start with Phase 1**: Review what the safety engineer produces on Day 1
2. **Move to Phase 2**: See how the board designer implements independently
3. **Examine Phase 3**: Understand how the tool validates everything
4. **Study the Gaps**: Learn how mismatches are identified and resolved

## Success Metrics

- Safety engineer productive from Day 1 ✓
- Board designer has implementation freedom ✓
- No rework due to late safety analysis ✓
- Complete traceability maintained ✓
- ISO 26262 compliance achieved ✓