# IBIS Comparison Notes - Important Clarification

## Current Status

After creating the IBIS examples and test cases, I need to clarify that the comparisons between GLACIER and eispice are based on:

1. **Documentation and Literature**: The limitations of eispice are derived from:
   - The SPISim blog stating "eispice... only supports simulating a rising waveform or a falling waveform, no repetition"
   - Stack Exchange discussions mentioning eispice has limited IBIS functionality
   - The fact that most free SPICE simulators "are totally lack of" IBIS support

2. **Not Direct Testing**: I have NOT actually run eispice with these specific examples to verify it fails. The claims about eispice limitations are based on documented capabilities, not empirical testing.

## What We Know For Certain

### GLACIER (Verified by Testing)
- DDR4 with ODT: Successfully finds 3 operating points (tested)
- PCIe sharp clamp: Handles 10x current jump without divergence (tested)
- Multi-driver contention: Solves for equilibrium point (tested)
- Performance: 247-1543 iterations, 1.2-7.7ms (measured)

### eispice (Based on Documentation)
- Limited to simple rise/fall waveforms
- No built-in support for multi-driver nets
- May struggle with sharp discontinuities
- Basic IBIS support but not comprehensive

## Recommended Paper Updates

To maintain academic integrity, the papers should be updated to clarify:

1. GLACIER results are from actual testing
2. eispice limitations are based on documented capabilities
3. Direct head-to-head testing was not performed

Example revised text:
"While eispice pioneered native IBIS support in open-source simulators, its documented limitations include support for only simple rise/fall waveforms and lack of multi-driver capability [cite]. GLACIER's testing demonstrates robust handling of these scenarios, though direct comparison testing with eispice was not performed."

## Future Work

For a truly rigorous comparison, we would need to:
1. Build eispice from source (possibly the Python 3 fork)
2. Create equivalent test circuits in eispice format
3. Run the same test cases and document actual failures/successes
4. Compare performance metrics directly

This would provide empirical evidence rather than relying on documented limitations.