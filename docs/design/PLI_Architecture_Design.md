# BHDL PLI Architecture Design

## Goal: High-Performance, Easy-to-Debug PLI with Minimal Overhead

### Core Architecture

```
┌─────────────────┐         ┌──────────────────┐
│   BHDL Engine   │ <-----> │ Behavioral Model │
│                 │  PLI    │   (Python/Rust)  │
│  - Electrical   │         │  - Algorithms    │
│  - Netlist      │         │  - State Machine │
│  - Timing       │         │  - Control       │
└─────────────────┘         └──────────────────┘
```

### 1. Communication Layer Design

#### Option A: Shared Memory (Recommended for Performance)

```rust
// BHDL side (Rust)
pub struct PLIChannel {
    // Shared memory segments
    pin_values: Arc<Mutex<MmapMut>>,      // Pin values
    parameters: Arc<Mutex<MmapMut>>,      // Static parameters
    waveforms: Arc<RwLock<MmapMut>>,      // Large waveform data
    
    // Control channel (small messages)
    control: Sender<PLICommand>,
    events: Receiver<PLIEvent>,
}

// Memory layout
struct PinValueBuffer {
    timestamp: f64,
    version: u64,  // For change detection
    values: [f64; MAX_PINS],
}
```

```python
# Python side
import mmap
import struct
import numpy as np

class PLIConnection:
    def __init__(self, shm_path):
        # Map shared memory
        self.shm_file = open(shm_path, 'r+b')
        self.pin_buffer = mmap.mmap(self.shm_file.fileno(), PIN_BUFFER_SIZE)
        
        # Zero-copy numpy array on shared memory
        self.pin_values = np.frombuffer(self.pin_buffer, dtype=np.float64)
        
    def read_pins_batch(self, count):
        # Read multiple timesteps at once - no serialization!
        return self.pin_values.reshape(-1, self.num_pins)[:count]
```

#### Option B: Domain Sockets (Easier Deployment)

```rust
// Use Cap'n Proto or FlatBuffers for zero-copy serialization
schema PLIMessage {
    union {
        pinUpdate: PinUpdate,
        paramUpdate: ParamUpdate,
        batchData: BatchData,
    }
}

struct BatchData {
    timesteps: [Timestep];  // Array of timesteps
    compressed: bool;       // Optional compression
}
```

### 2. Execution Models

#### Lockstep Mode (Default - Easy Debugging)

```rust
impl PLIExecutor {
    fn step_lockstep(&mut self, dt: f64) {
        // 1. Send current state to model
        self.send_pin_values();
        
        // 2. Wait for model to calculate
        let result = self.receive_model_output();
        
        // 3. Apply results
        self.apply_model_outputs(result);
    }
}
```

#### Batch Mode (10-1000x Performance)

```rust
impl PLIExecutor {
    fn step_batch(&mut self, dt: f64, batch_size: usize) {
        // 1. Prepare batch of timesteps
        let batch = self.prepare_batch(dt, batch_size);
        
        // 2. Send entire batch
        self.send_batch(batch);
        
        // 3. Model processes all at once
        let results = self.receive_batch_results();
        
        // 4. Apply results over time
        for (i, result) in results.iter().enumerate() {
            self.schedule_at(i as f64 * dt, result);
        }
    }
}
```

#### Async Mode (Maximum Performance)

```rust
impl PLIExecutor {
    async fn run_async(&mut self) {
        // Model runs in separate thread/process
        let model_handle = tokio::spawn(async {
            self.model.run_continuous()
        });
        
        // Synchronize at specific points
        while let Some(sync_point) = self.next_sync_point() {
            self.advance_to(sync_point).await;
            self.synchronize_with_model().await;
        }
    }
}
```

### 3. Debugging Support

#### Integrated Debugger Protocol

```python
# Python model with debug support
class DebuggableBuckModel(BehavioralModel):
    def __init__(self):
        self.debugger = PLIDebugger(port=12345)
        self.breakpoints = {}
        
    def step(self, dt):
        # Check breakpoints
        if self.debugger.should_break(self.state):
            self.debugger.enter_debug_mode(self.get_debug_context())
            
        # Normal execution
        self.update_state(dt)
        
    def get_debug_context(self):
        return {
            'state': self.state,
            'locals': locals(),
            'waveforms': self.get_recent_waveforms(),
            'call_stack': traceback.extract_stack()
        }
```

#### Time-Travel Debugging

```rust
struct PLIDebugger {
    // Ring buffer of states for rewind
    history: VecDeque<SystemState>,
    max_history: usize,
    
    // Checkpoint system for long runs
    checkpoints: BTreeMap<f64, SystemCheckpoint>,
}

impl PLIDebugger {
    fn rewind_to(&mut self, time: f64) {
        // Find nearest checkpoint
        let checkpoint = self.checkpoints.range(..=time).last();
        
        // Restore and replay
        self.restore_checkpoint(checkpoint);
        self.replay_to(time);
    }
}
```

### 4. Language Bindings

#### Python (via PyO3)

```rust
#[pyclass]
struct BehavioralModel {
    #[pyo3(get, set)]
    name: String,
    pins: HashMap<String, Pin>,
}

#[pymethods]
impl BehavioralModel {
    fn read_pin(&self, name: &str) -> PyResult<f64> {
        self.pins.get(name)
            .map(|p| p.value)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyKeyError, _>(name))
    }
    
    #[args(batch_size = "1")]
    fn step_batch(&mut self, dt: f64, batch_size: usize) -> PyResult<Vec<f64>> {
        // Efficient batch processing
    }
}
```

#### Rust (Native Performance)

```rust
// Direct FFI for Rust models
#[repr(C)]
pub struct PLIInterface {
    version: u32,
    read_pin: extern "C" fn(*const c_char) -> f64,
    write_pin: extern "C" fn(*const c_char, f64),
    get_time: extern "C" fn() -> f64,
}

pub trait RustBehavioralModel {
    fn init(&mut self, pli: &PLIInterface);
    fn step(&mut self, dt: f64);
}
```

#### C/C++ (Legacy Integration)

```c
// C API for legacy models
typedef struct {
    double (*read_pin)(const char* name);
    void (*write_pin)(const char* name, double value);
    void (*log_event)(const char* msg);
} bhdl_pli_t;

void model_step(bhdl_pli_t* pli, double dt) {
    double vin = pli->read_pin("VIN");
    // Model logic...
    pli->write_pin("VOUT", vout);
}
```

### 5. Performance Optimizations

#### Pin Change Detection

```rust
struct OptimizedPLI {
    // Only send changed pins
    pin_versions: HashMap<PinId, u64>,
    
    fn send_pin_updates(&mut self) {
        let mut updates = Vec::new();
        
        for (id, pin) in &self.pins {
            if pin.version > self.pin_versions[id] {
                updates.push((id, pin.value));
                self.pin_versions[id] = pin.version;
            }
        }
        
        if !updates.is_empty() {
            self.channel.send_updates(updates);
        }
    }
}
```

#### Predictive Execution

```python
class PredictiveBuckModel(BehavioralModel):
    def predict_next_states(self, dt, horizon):
        """Predict multiple future states for async execution"""
        states = []
        state = self.current_state.copy()
        
        for _ in range(horizon):
            state = self.step_internal(state, dt)
            states.append(state)
            
        return states
```

### 6. Error Handling

```rust
enum PLIError {
    ModelCrashed { reason: String, backtrace: String },
    Timeout { elapsed: Duration },
    ProtocolMismatch { expected: u32, got: u32 },
    InvalidPin { name: String },
}

impl PLIExecutor {
    fn safe_step(&mut self, dt: f64) -> Result<(), PLIError> {
        // Catch model crashes
        match self.try_step(dt).timeout(Duration::from_millis(100)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                self.handle_model_error(e);
                Err(PLIError::ModelCrashed { ... })
            }
            Err(_) => {
                self.kill_model();
                Err(PLIError::Timeout { ... })
            }
        }
    }
}
```

### 7. Configuration and Discovery

```toml
# .bhdl/pli_config.toml

[models.buck_controller]
language = "python"
path = "models/buck.py"
class = "BuckController"
requires = ["numpy>=1.20", "scipy>=1.7"]

[models.motor_foc]
language = "rust"
path = "target/release/libmotor_foc.so"
interface = "bhdl_pli_v1"

[execution]
default_mode = "batch"
batch_size = 1000
shared_memory_size = "10MB"

[debug]
enable_history = true
history_size = 10000
checkpoint_interval = "1ms"
```

### 8. Testing Support

```python
# Test harness for PLI models
class PLIModelTest(unittest.TestCase):
    def setUp(self):
        self.model = BuckController()
        self.mock_pli = MockPLIInterface()
        
    def test_soft_start(self):
        # Set up initial conditions
        self.mock_pli.set_pin("VIN", 12.0)
        self.mock_pli.set_pin("ENABLE", 1.0)
        
        # Run for 10ms
        for _ in range(10000):
            self.model.step(1e-6)
            
        # Check soft start completed
        self.assertAlmostEqual(
            self.mock_pli.get_pin("VOUT"), 
            3.3, 
            places=1
        )
```

## Summary of PLI Design Choices

1. **Shared memory** for performance-critical paths
2. **Batch processing** to amortize overhead
3. **Multiple execution modes** for different use cases
4. **Integrated debugging** with time-travel
5. **Zero-copy data transfer** where possible
6. **Language-native bindings** (not just pipes)
7. **Predictive execution** for async performance
8. **Robust error handling** with recovery

This design minimizes the PLI overhead while maintaining ease of use and debugging capabilities!