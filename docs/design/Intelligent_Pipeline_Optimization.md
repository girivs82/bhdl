# Intelligent Pipeline Optimization for Hierarchical Modules

## Overview

The pipeline should be smart about identical entity instances to avoid redundant work and provide meaningful naming. This affects synthesis, analysis, and simulation.

## 1. Reference Designator Intelligence

### Current Approach (Naive)
```
board System {
    buck1: BuckConverter { ... }  
    buck2: BuckConverter { ... }
}

// Current: Random unique names
R1, R2, R3, C1, C2, L1  // buck1 components
R4, R5, R6, C3, C4, L2  // buck2 components (confusing!)
```

### Intelligent Approach
```
// Smart hierarchical naming
buck1.R1, buck1.R2, buck1.C1, buck1.L1
buck2.R1, buck2.R2, buck2.C1, buck2.L1

// Or flattened with instance prefix
R1_1, R2_1, C1_1, L1_1  // buck1
R1_2, R2_2, C1_2, L1_2  // buck2
```

### Implementation

```rust
// In synthesizer
pub struct RefDesAllocator {
    // Track refdes per module type
    module_refdes: HashMap<ModuleTypeId, RefDesMap>,
    // Instance counters
    instance_counters: HashMap<String, u32>,
}

impl RefDesAllocator {
    pub fn allocate_for_instance(
        &mut self,
        module_type: &ModuleType,
        instance_path: &InstancePath,
        component: &Component,
    ) -> String {
        // Get module-local refdes (R1, C1, etc.)
        let local_refdes = self.get_local_refdes(module_type, component);
        
        // Get instance number
        let instance_num = self.get_instance_number(instance_path);
        
        // Combine: R1_1, R1_2, etc.
        format!("{}_{}", local_refdes, instance_num)
    }
    
    fn get_local_refdes(&mut self, module_type: &ModuleType, component: &Component) -> String {
        let refdes_map = self.module_refdes
            .entry(module_type.id())
            .or_insert_with(RefDesMap::new);
            
        refdes_map.allocate(component.type_prefix())
    }
}
```

## 2. Module Instance Deduplication

### Concept
```bhdl
board PowerSupply {
    // These three are IDENTICAL
    rail_5v: BuckConverter(vout=5V) { ... }
    rail_5v_aux: BuckConverter(vout=5V) { ... }
    rail_5v_backup: BuckConverter(vout=5V) { ... }
    
    // This one is DIFFERENT
    rail_3v3: BuckConverter(vout=3.3V) { ... }
}
```

### Instance Signature
```rust
#[derive(Hash, Eq, PartialEq)]
pub struct ModuleSignature {
    module_type: String,
    parameters: BTreeMap<String, Value>,  // Sorted for consistency
}

pub struct ModuleCache {
    // Cache analyzed modules by signature
    analyzed: HashMap<ModuleSignature, AnalyzedModule>,
}

impl ModuleCache {
    pub fn get_or_analyze(
        &mut self,
        module: &Module,
        params: &HashMap<String, Value>,
    ) -> &AnalyzedModule {
        let signature = ModuleSignature {
            module_type: module.name().to_string(),
            parameters: params.iter().collect(),  // Sorted
        };
        
        self.analyzed.entry(signature)
            .or_insert_with(|| analyze_module(module, params))
    }
}
```

## 3. SPICE Analysis Optimization

### Smart Safety Analysis
```rust
impl SpiceAnalyzer {
    pub fn analyze_board(&mut self, board: &Board) -> SafetyReport {
        let mut module_results = HashMap::new();
        let mut instance_contexts = Vec::new();
        
        // Group instances by signature
        let instance_groups = self.group_by_signature(&board.instances);
        
        for (signature, instances) in instance_groups {
            if instances.len() == 1 {
                // Unique instance - analyze normally
                let result = self.analyze_instance(&instances[0]);
                instance_contexts.push(result);
            } else {
                // Multiple identical instances
                if let Some(cached) = module_results.get(&signature) {
                    // Reuse analysis
                    for instance in instances {
                        instance_contexts.push(
                            self.apply_cached_analysis(cached, instance)
                        );
                    }
                } else {
                    // Analyze first instance
                    let result = self.analyze_instance(&instances[0]);
                    module_results.insert(signature.clone(), result.clone());
                    
                    // Apply to all instances
                    instance_contexts.push(result);
                    for instance in &instances[1..] {
                        instance_contexts.push(
                            self.apply_cached_analysis(&result, instance)
                        );
                    }
                }
            }
        }
        
        self.combine_results(instance_contexts)
    }
}
```

### Context-Aware Analysis
```rust
// Some analyses need instance context
impl SpiceAnalyzer {
    fn apply_cached_analysis(
        &self,
        cached: &ModuleAnalysis,
        instance: &Instance,
    ) -> InstanceAnalysis {
        // Most safety checks are identical
        let mut result = cached.clone();
        
        // But update context-dependent checks
        result.update_context(InstanceContext {
            input_voltage: self.get_pin_voltage(&instance, "VIN"),
            load_current: self.get_pin_current(&instance, "VOUT"),
            ambient_temp: self.board_ambient_temp,
        });
        
        result
    }
}
```

## 4. Hierarchical Netlist Optimization

### Netlist Structure
```rust
pub struct HierarchicalNetlist {
    // Module definitions (deduplicated)
    modules: HashMap<ModuleSignature, ModuleDefinition>,
    
    // Top-level instances
    instances: Vec<ModuleInstance>,
    
    // Flattened view (lazy)
    flat_view: OnceCell<FlatNetlist>,
}

pub struct ModuleInstance {
    name: String,
    signature: ModuleSignature,
    connections: HashMap<PinName, NetId>,
    position: Point,  // For layout
}

impl HierarchicalNetlist {
    pub fn add_instance(&mut self, name: String, module: Module, params: Params) {
        let signature = ModuleSignature::new(&module, &params);
        
        // Only synthesize if new signature
        if !self.modules.contains_key(&signature) {
            let definition = synthesize_module(&module, &params);
            self.modules.insert(signature.clone(), definition);
        }
        
        self.instances.push(ModuleInstance {
            name,
            signature,
            connections: HashMap::new(),
            position: Point::default(),
        });
    }
}
```

## 5. Layout Intelligence

### Repeated Module Layout
```rust
impl LayoutEngine {
    pub fn layout_repeated_modules(&mut self, instances: &[ModuleInstance]) {
        // Group by signature
        let groups = group_by_signature(instances);
        
        for (signature, instances) in groups {
            if instances.len() > 1 {
                // Layout once, replicate
                let template_layout = self.layout_module(&signature);
                
                // Arrange instances in grid
                let grid = self.calculate_grid(instances.len());
                
                for (idx, instance) in instances.iter().enumerate() {
                    let (row, col) = grid.position(idx);
                    let offset = Point::new(
                        col as f64 * (template_layout.width + SPACING),
                        row as f64 * (template_layout.height + SPACING),
                    );
                    
                    self.place_module_instance(
                        instance,
                        &template_layout,
                        offset,
                    );
                }
            } else {
                // Single instance - normal layout
                self.layout_module_instance(&instances[0]);
            }
        }
    }
}
```

## 6. Simulation Optimization

### Behavioral Model Caching
```rust
impl BehavioralSimulator {
    // Cache initialized behavioral models
    model_cache: HashMap<ModuleSignature, Box<dyn BehavioralModel>>,
    
    pub fn get_model(&mut self, signature: &ModuleSignature) -> &mut dyn BehavioralModel {
        self.model_cache
            .entry(signature.clone())
            .or_insert_with(|| {
                create_behavioral_model(&signature)
            })
    }
    
    pub fn step_all(&mut self, dt: f64) {
        // Group instances by model
        let instance_groups = self.group_by_model();
        
        for (model_sig, instances) in instance_groups {
            if instances.len() > 1 {
                // Batch process identical models
                let model = self.get_model(&model_sig);
                let batch_inputs = collect_batch_inputs(&instances);
                let batch_outputs = model.step_batch(dt, batch_inputs);
                distribute_batch_outputs(&instances, batch_outputs);
            } else {
                // Single instance
                let model = self.get_model(&model_sig);
                let inputs = collect_inputs(&instances[0]);
                let outputs = model.step(dt, inputs);
                apply_outputs(&instances[0], outputs);
            }
        }
    }
}
```

## 7. Generate-Aware Optimization

### Array Instance Handling
```bhdl
entity MultiPhase(phases: int = 4) {
    generate for i in 0..phases {
        phase[i]: PhaseController { ... }
    }
}
```

```rust
impl Synthesizer {
    fn synthesize_generate_array(&mut self, gen: &GenerateFor) {
        let base_name = gen.array_name();
        let module_sig = self.evaluate_module_signature(&gen.body);
        
        // Create array metadata
        self.netlist.add_array(ArrayInstance {
            base_name: base_name.clone(),
            signature: module_sig,
            count: gen.range.count(),
            dimensions: vec![gen.range.count()],
        });
        
        // Don't synthesize each element separately!
        // The netlist knows it's an array
    }
}

// In SPICE analyzer
impl SpiceAnalyzer {
    fn analyze_array(&mut self, array: &ArrayInstance) {
        // Analyze one representative element
        let representative = self.analyze_module(&array.signature);
        
        // Check if elements interact
        if self.has_coupling(&array) {
            // Full analysis needed
            self.analyze_coupled_array(&array, &representative);
        } else {
            // Can reuse analysis
            self.results.set_array_result(&array, representative);
        }
    }
}
```

## 8. Diff-Based Re-analysis

### Incremental Analysis
```rust
pub struct IncrementalAnalyzer {
    previous_results: HashMap<ModuleSignature, AnalysisResult>,
    
    pub fn analyze_incremental(&mut self, netlist: &Netlist) -> AnalysisResult {
        let mut results = AnalysisResult::new();
        
        for instance in &netlist.instances {
            let sig = instance.signature();
            
            if let Some(prev) = self.previous_results.get(&sig) {
                if !instance.has_changed_since(prev.timestamp) {
                    // Reuse previous analysis
                    results.add_cached(instance, prev);
                    continue;
                }
            }
            
            // Fresh analysis needed
            let result = self.analyze_instance(instance);
            self.previous_results.insert(sig, result.clone());
            results.add_fresh(instance, result);
        }
        
        results
    }
}
```

## Benefits

1. **Performance**: Analyze each unique entity once
2. **Clarity**: R1_1, R1_2 clearly shows related components  
3. **Debugging**: Easy to trace issues to specific instances
4. **Memory**: Share analysis results and layouts
5. **Correctness**: Consistent analysis for identical entities

## Implementation Priority

1. **Phase 1**: Reference designator intelligence
2. **Phase 2**: Module signature and caching  
3. **Phase 3**: SPICE analysis deduplication
4. **Phase 4**: Layout template replication
5. **Phase 5**: Behavioral model batching