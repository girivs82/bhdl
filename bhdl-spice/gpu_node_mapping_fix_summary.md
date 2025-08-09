# GPU Node Mapping Bug Fix Summary

## Issues Found

### 1. Non-deterministic Node Ordering
**Problem**: The GPU converter was iterating over `circuit.nodes()` directly, which returns nodes in the order they exist in the petgraph. This order is not deterministic and may not match the NodeIndex values.

**Fix**: Sort nodes by their NodeIndex before mapping to ensure consistent ordering:
```rust
let mut nodes: Vec<(NodeIndex, &crate::circuit::Node)> = circuit.nodes().collect();
nodes.sort_by_key(|(idx, _)| idx.index());
```

### 2. HashMap Iteration for Variables
**Problem**: When creating voltage variables, the code iterated over `&self.node_map` (a HashMap), which has non-deterministic iteration order. This could cause variables to be created in different orders between runs.

**Fix**: Sort the nodes by their GPU index before creating variables:
```rust
let mut sorted_nodes: Vec<(&NodeIndex, &u32)> = self.node_map.iter().collect();
sorted_nodes.sort_by_key(|(_, &gpu_idx)| gpu_idx);
```

### 3. Debug Logging Added
Added debug logging to help diagnose node mapping issues:
- Log each node mapping: Circuit NodeIndex -> GPU index
- Log each component connection mapping
- This helps verify the mapping is correct

## How to Debug

1. Run the debug test with logging enabled:
```bash
RUST_LOG=debug cargo run --features gpu --bin debug_gpu_node_mapping
```

2. The test will show:
   - Original NodeIndex values from the circuit
   - GPU index assignments
   - Component connections with both original and GPU indices
   - Verification of expected vs actual mappings

## Expected Behavior

For a circuit: VDD -> R1 -> n1 -> LED1 -> n2 -> LED2 -> GND

The mapping should be:
- VDD: NodeIndex 0 -> GPU index 0
- n1: NodeIndex 1 -> GPU index 1  
- n2: NodeIndex 2 -> GPU index 2
- GND: NodeIndex 3 -> GPU index 3

And components should connect:
- V1: GPU node 0 -> GPU node 3 (VDD -> GND)
- R1: GPU node 0 -> GPU node 1 (VDD -> n1)
- D1: GPU node 1 -> GPU node 2 (n1 -> n2)
- D2: GPU node 2 -> GPU node 3 (n2 -> GND)

## Root Cause

The GPU solver was getting incorrect solutions because:
1. Node indices were scrambled during conversion
2. The ground node might not have been correctly identified
3. Components were connected to wrong nodes in the GPU representation

This caused the solver to solve a different circuit topology than intended, leading to physically impossible solutions like:
- n1 (connection between R1 and LED1) being at 0V
- Negative voltage drops across LEDs
- Incorrect current flow