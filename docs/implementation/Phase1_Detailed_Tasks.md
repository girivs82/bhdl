# Phase 1: Core Simulation Infrastructure - Detailed Tasks

## 1.1 Basic Simulation Engine

### Task 1.1.1: Create Project Structure
```
bhdl-sim/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── time.rs
│   │   ├── state.rs
│   │   └── control.rs
│   ├── circuit/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   └── loader.rs
│   ├── evaluation/
│   │   ├── mod.rs
│   │   └── scheduler.rs
│   └── error.rs
```

### Task 1.1.2: Time Management System
```rust
// bhdl-sim/src/engine/time.rs
pub struct TimeManager {
    current_time: f64,
    time_step: f64,
    min_time_step: f64,
    max_time_step: f64,
    adaptive: bool,
}

impl TimeManager {
    pub fn new(time_step: f64) -> Self;
    pub fn advance(&mut self) -> f64;
    pub fn set_adaptive(&mut self, adaptive: bool);
    pub fn suggest_time_step(&self, error: f64) -> f64;
}
```

### Task 1.1.3: Simulation State Machine
```rust
// bhdl-sim/src/engine/state.rs
pub enum SimulationState {
    Idle,
    Initializing,
    Running,
    Paused,
    Stepping,
    Completed,
    Error(SimulationError),
}

pub struct StateMachine {
    state: SimulationState,
    transitions: HashMap<(SimulationState, Command), SimulationState>,
}
```

### Task 1.1.4: Control Interface
```rust
// bhdl-sim/src/engine/control.rs
pub struct SimulationControl {
    commands: mpsc::Receiver<Command>,
    responses: mpsc::Sender<Response>,
}

pub enum Command {
    Start,
    Stop,
    Pause,
    Resume,
    Step(usize), // Number of steps
    SetBreakpoint(Breakpoint),
    RemoveBreakpoint(BreakpointId),
}
```

### Task 1.1.5: Configuration System
```rust
// bhdl-sim/src/engine/config.rs
#[derive(Serialize, Deserialize)]
pub struct SimulationConfig {
    pub time_step: f64,
    pub max_time: f64,
    pub convergence_threshold: f64,
    pub max_iterations: usize,
    pub output_config: OutputConfig,
    pub performance: PerformanceConfig,
}

pub struct PerformanceConfig {
    pub parallel_evaluation: bool,
    pub cache_expressions: bool,
    pub batch_size: usize,
}
```

## 1.2 Circuit State Management

### Task 1.2.1: State Representation
```rust
// bhdl-sim/src/circuit/state.rs
pub struct CircuitState {
    // Time-invariant data
    topology: CircuitTopology,
    
    // Time-variant data
    attributes: AttributeStorage,
    pins: PinStorage,
    nets: NetStorage,
    
    // Metadata
    dirty_flags: DirtyFlags,
    change_log: ChangeLog,
}

pub struct AttributeStorage {
    values: HashMap<AttributeId, RuntimeValue>,
    previous: HashMap<AttributeId, RuntimeValue>, // For change detection
}
```

### Task 1.2.2: State Initialization
```rust
// bhdl-sim/src/circuit/loader.rs
pub struct CircuitLoader {
    netlist: Netlist,
    analysis_result: AnalysisResult,
}

impl CircuitLoader {
    pub fn load_circuit(&self) -> Result<CircuitState, LoadError> {
        // 1. Create topology from netlist
        // 2. Initialize attribute values
        // 3. Set up pin models
        // 4. Validate initial state
    }
}
```

### Task 1.2.3: State Update Mechanism
```rust
// bhdl-sim/src/circuit/state.rs
impl CircuitState {
    pub fn begin_timestep(&mut self);
    pub fn update_attribute(&mut self, id: AttributeId, value: RuntimeValue);
    pub fn update_pin(&mut self, id: PinId, value: PinValue);
    pub fn commit_timestep(&mut self);
    pub fn rollback_timestep(&mut self);
}
```

### Task 1.2.4: State Persistence
```rust
// bhdl-sim/src/circuit/snapshot.rs
pub struct StateSnapshot {
    time: f64,
    attributes: HashMap<AttributeId, RuntimeValue>,
    pins: HashMap<PinId, PinValue>,
    compressed: bool,
}

impl CircuitState {
    pub fn create_snapshot(&self) -> StateSnapshot;
    pub fn restore_snapshot(&mut self, snapshot: &StateSnapshot);
    pub fn save_to_file(&self, path: &Path) -> Result<(), io::Error>;
}
```

## 1.3 Attribute Evaluation Integration

### Task 1.3.1: Evaluation Context Bridge
```rust
// bhdl-sim/src/evaluation/context.rs
pub struct SimulationEvaluationContext<'a> {
    circuit_state: &'a CircuitState,
    time_manager: &'a TimeManager,
    builtin_manager: BuiltinVariableManager,
}

impl<'a> SimulationEvaluationContext<'a> {
    pub fn create_eval_context(&self) -> EvaluationContext;
    pub fn sync_results(&mut self, results: &EvaluationResults);
}
```

### Task 1.3.2: Dependency-Based Scheduler
```rust
// bhdl-sim/src/evaluation/scheduler.rs
pub struct EvaluationScheduler {
    dependency_graph: DependencyGraph,
    evaluation_order: Vec<AttributeId>,
    dirty_set: HashSet<AttributeId>,
}

impl EvaluationScheduler {
    pub fn mark_dirty(&mut self, attr: AttributeId);
    pub fn get_evaluation_batch(&self) -> Vec<AttributeId>;
    pub fn update_dependencies(&mut self, changes: &[DependencyChange]);
}
```

### Task 1.3.3: When Block Processor
```rust
// bhdl-sim/src/evaluation/when_processor.rs
pub struct WhenBlockProcessor {
    blocks: Vec<WhenBlockInfo>,
    condition_cache: HashMap<WhenBlockId, bool>,
}

pub struct WhenBlockInfo {
    id: WhenBlockId,
    condition: Expr,
    assignments: Vec<Assignment>,
    last_state: bool,
}

impl WhenBlockProcessor {
    pub fn evaluate_conditions(&mut self, ctx: &EvaluationContext);
    pub fn get_triggered_blocks(&self) -> Vec<&WhenBlockInfo>;
    pub fn apply_assignments(&self, block: &WhenBlockInfo, state: &mut CircuitState);
}
```

### Task 1.3.4: Error Recovery
```rust
// bhdl-sim/src/evaluation/error_handler.rs
pub struct EvaluationErrorHandler {
    error_policy: ErrorPolicy,
    error_log: Vec<EvaluationError>,
}

pub enum ErrorPolicy {
    StopOnError,
    ContinueWithDefault(RuntimeValue),
    ContinueWithLastValue,
    PropagateError,
}

impl EvaluationErrorHandler {
    pub fn handle_error(&mut self, error: EvaluationError) -> ErrorRecovery;
    pub fn should_abort(&self) -> bool;
}
```

## Testing Strategy for Phase 1

### Unit Tests
1. **Time Manager**: Step advancement, adaptive stepping
2. **State Machine**: All state transitions
3. **Circuit State**: CRUD operations, snapshots
4. **Evaluation**: Expression evaluation, dependencies
5. **When Blocks**: Condition evaluation, assignments

### Integration Tests
1. **Simple RC Circuit**: Basic time stepping
2. **Logic Gates**: Digital signal propagation
3. **Behavioral Counter**: State persistence
4. **Error Scenarios**: Recovery and reporting

### Performance Tests
1. **Large Circuits**: 10k+ components
2. **Complex Expressions**: Deep nesting
3. **Many When Blocks**: 1000+ conditions
4. **Long Simulations**: 1M+ timesteps

## Deliverables for Phase 1

1. **Working Simulation Engine**
   - Can load a circuit
   - Advances time correctly
   - Evaluates attributes
   - Handles when blocks

2. **Basic Test Suite**
   - Unit tests > 80% coverage
   - Integration tests pass
   - Performance benchmarks established

3. **Documentation**
   - API documentation
   - Architecture diagram
   - Usage examples

4. **Demo Application**
   - Simple CLI tool
   - RC circuit simulation
   - Waveform output to CSV

## Success Criteria

- [ ] Can simulate `test_behavioral_complete.bhdl`
- [ ] Time stepping is accurate to 1e-12
- [ ] Attribute evaluation matches expected values
- [ ] When blocks trigger correctly
- [ ] Performance: 10k evaluations/second minimum
- [ ] Memory usage < 100MB for 1k component circuit
- [ ] All tests passing
- [ ] Documentation complete