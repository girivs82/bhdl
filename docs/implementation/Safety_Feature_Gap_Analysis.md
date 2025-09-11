# BHDL Safety Feature Gap Analysis
## What We Need to Implement for Full Safety Support

Based on our BCM example project, here's what needs to be added to BHDL to support the complete safety workflow:

## 1. Parser Enhancements

### 1.1 `satisfies` Keyword
```rust
// In bhdl-parser/src/parser.rs
// Need to add parsing for satisfies blocks

satisfies {
    REQ_001: via component_name;
    REQ_002: {
        implementation: "description";
        evidence: "test report";
    }
}
```

**Implementation Plan:**
- Add `SATISFIES_KW` token
- Parse satisfies block similar to attributes
- Support both simple (via) and detailed forms
- Store in AST as SatisfiesClause

### 1.2 Safety-Specific Constructs
```rust
// Need to parse these safety-specific declarations
system_safety Name { ... }
subsystem_safety Name { ... }
hazards { ... }
safety_goals { ... }
functional_safety_requirements { ... }
```

**Implementation Plan:**
- Add safety keywords to lexer
- Create safety-specific AST nodes
- Parse hierarchical safety structures

## 2. AST Extensions

### 2.1 Safety AST Nodes
```rust
// In bhdl-ast/src/safety.rs (new file)

pub struct SystemSafety {
    name: Ident,
    context: SystemContext,
    hazards: Vec<Hazard>,
    safety_goals: Vec<SafetyGoal>,
    requirements: Vec<SafetyRequirement>,
}

pub struct Hazard {
    id: String,
    description: String,
    asil: AsilLevel,
    severity: Severity,
    exposure: Exposure,
    controllability: Controllability,
}

pub struct SafetyRequirement {
    id: String,
    description: String,
    asil: AsilLevel,
    allocation: String,
    requirement: RequirementSpec,
}

pub struct SatisfiesClause {
    requirements: HashMap<String, SatisfiesSpec>,
}
```

## 3. Standard Library Safety Types

### 3.1 Safety Attributes
```bhdl
// In bhdl-stdlib/attributes/safety.bhdl

attribute_type asil: enum {
    values: [QM, ASIL_A, ASIL_B, ASIL_C, ASIL_D];
    description: "ISO 26262 ASIL level";
}

attribute_type spfm: percentage {
    range: 0%..100%;
    description: "Single Point Fault Metric";
}

attribute_type lfm: percentage {
    range: 0%..100%;
    description: "Latent Fault Metric";
}

attribute_type pmhf: fit_rate {
    unit: FIT;
    range: 0..∞;
    description: "Probabilistic Metric for Hardware Failures";
}

attribute_type diagnostic_coverage: percentage {
    range: 0%..100%;
    description: "Diagnostic coverage percentage";
}
```

### 3.2 Safety Capabilities (Traits)
```bhdl
// In bhdl-stdlib/capabilities/safety.bhdl

capability VoltageMonitoring {
    monitors: [voltage];
    thresholds: [undervoltage, overvoltage];
    response_time: time;
    coverage: percentage;
}

capability OvervoltageProtection {
    threshold: voltage;
    response_time: time;
    protection_method: enum[shutdown, clamp, crowbar];
}

capability DiagnosticReadback {
    signals: [signal];
    update_rate: frequency;
    accuracy: percentage;
}
```

## 4. Analyzer Enhancements

### 4.1 Safety Analysis Pass (Pass 9)
```rust
// In bhdl-analyzer/src/passes/safety_analysis.rs (new)

pub struct SafetyAnalysisPass;

impl SafetyAnalysisPass {
    pub fn analyze(&mut self, ast: &AST) -> SafetyAnalysisResult {
        // 1. Collect all safety requirements
        let requirements = self.collect_requirements(ast);
        
        // 2. Collect all satisfies declarations
        let satisfies = self.collect_satisfies(ast);
        
        // 3. Check requirement coverage
        let coverage = self.check_coverage(&requirements, &satisfies);
        
        // 4. Calculate safety metrics
        let metrics = self.calculate_metrics(ast);
        
        // 5. Generate traceability matrix
        let traceability = self.generate_traceability(&requirements, &satisfies);
        
        SafetyAnalysisResult {
            requirements,
            coverage,
            metrics,
            traceability,
            violations: self.violations,
        }
    }
}
```

### 4.2 Requirement Coverage Checking
```rust
impl SafetyAnalysisPass {
    fn check_coverage(&mut self, 
                     requirements: &[Requirement], 
                     satisfies: &[SatisfiesDecl]) -> Coverage {
        // For each requirement, check if satisfied
        for req in requirements {
            if !self.is_satisfied(req, satisfies) {
                self.violations.push(Diagnostic::error(
                    format!("Safety requirement {} not satisfied", req.id)
                ));
            }
        }
        
        // Calculate coverage percentage
        let covered = satisfies.len();
        let total = requirements.len();
        Coverage {
            percentage: (covered as f32 / total as f32) * 100.0,
            uncovered: self.find_uncovered(requirements, satisfies),
        }
    }
}
```

## 5. Synthesizer Enhancements

### 5.1 Safety Metric Calculation
```rust
// In bhdl-synthesizer/src/safety_metrics.rs (new)

pub struct SafetyMetricsCalculator;

impl SafetyMetricsCalculator {
    pub fn calculate_system_metrics(&self, netlist: &Netlist) -> SystemMetrics {
        // 1. Roll up component FIT rates
        let total_fit = self.sum_fit_rates(netlist);
        
        // 2. Calculate SPFM from diagnostic coverage
        let spfm = self.calculate_spfm(netlist);
        
        // 3. Calculate LFM from latent fault coverage
        let lfm = self.calculate_lfm(netlist);
        
        // 4. Determine achieved ASIL
        let asil = self.determine_asil(spfm, lfm, total_fit);
        
        SystemMetrics {
            pmhf: total_fit,
            spfm,
            lfm,
            achieved_asil: asil,
        }
    }
}
```

### 5.2 Safety Documentation Generation
```rust
// In bhdl-synthesizer/src/safety_docs.rs (new)

pub struct SafetyDocGenerator;

impl SafetyDocGenerator {
    pub fn generate_safety_case(&self, 
                                analysis: &SafetyAnalysisResult,
                                netlist: &Netlist) -> SafetyCase {
        SafetyCase {
            requirements_traceability: self.generate_rtm(analysis),
            fmea: self.generate_fmea(netlist),
            metrics_summary: self.generate_metrics(analysis),
            validation_plan: self.generate_validation(analysis),
        }
    }
}
```

## 6. Component Database Extensions

### 6.1 Safety Properties in Database
```sql
-- Add safety columns to components table
ALTER TABLE components ADD COLUMN fit_rate REAL;
ALTER TABLE components ADD COLUMN asil_capability TEXT;
ALTER TABLE components ADD COLUMN diagnostic_features TEXT;

-- Safety mechanisms table
CREATE TABLE safety_mechanisms (
    id INTEGER PRIMARY KEY,
    component_id INTEGER REFERENCES components(id),
    mechanism_type TEXT,
    coverage_percentage REAL,
    response_time_us REAL
);
```

### 6.2 Component Safety Attributes
```rust
// In bhdl-components/src/models.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentSafety {
    pub fit_rate: Option<f64>,
    pub asil_capability: Option<AsilLevel>,
    pub diagnostic_coverage: Option<f64>,
    pub safety_mechanisms: Vec<SafetyMechanism>,
}
```

## 7. Compiler Integration

### 7.1 Safety-Aware Compilation
```rust
// In main compilation pipeline

pub fn compile_with_safety(source: &str) -> Result<SafetyAwareOutput> {
    // 1. Parse including safety constructs
    let ast = parser.parse_with_safety(source)?;
    
    // 2. Run standard analysis passes
    let analysis = analyzer.analyze(&ast)?;
    
    // 3. Run safety analysis pass
    let safety = safety_analyzer.analyze(&ast, &analysis)?;
    
    // 4. Check safety constraints
    safety_checker.check_constraints(&safety)?;
    
    // 5. Generate netlist with safety metadata
    let netlist = synthesizer.generate_with_safety(&ast, &analysis, &safety)?;
    
    // 6. Calculate final metrics
    let metrics = metrics_calculator.calculate(&netlist)?;
    
    // 7. Generate documentation
    let docs = doc_generator.generate(&safety, &metrics)?;
    
    Ok(SafetyAwareOutput {
        netlist,
        safety_analysis: safety,
        metrics,
        documentation: docs,
    })
}
```

## 8. Missing Features Priority

### High Priority (Needed for Basic Safety)
1. ✅ Parser: `satisfies` keyword
2. ✅ AST: Safety requirement nodes
3. ✅ Stdlib: Safety attribute types
4. ✅ Analyzer: Basic requirement coverage checking

### Medium Priority (Enhanced Safety Analysis)
5. ⚠️ Synthesizer: Metric calculation
6. ⚠️ Analyzer: Safety analysis pass
7. ⚠️ Database: Safety properties
8. ⚠️ Documentation generation

### Low Priority (Nice to Have)
9. ○ IDE support for safety
10. ○ Graphical safety reports
11. ○ FMEA automation
12. ○ Safety case templates

## Implementation Sequence

### Phase 1: Core Language Support (Week 1)
- Add `satisfies` keyword to parser
- Create safety AST nodes
- Define safety attributes in stdlib
- Basic parsing test cases

### Phase 2: Analysis Support (Week 2)
- Implement safety analysis pass
- Add requirement coverage checking
- Create diagnostic messages
- Integration with existing analyzer

### Phase 3: Synthesis Support (Week 3)
- Safety metric calculation
- Traceability matrix generation
- Safety documentation output
- Integration tests

### Phase 4: Tooling (Week 4)
- Component database extensions
- CLI commands for safety
- Report generation
- Example projects

## Testing Strategy

### Unit Tests
- Parser tests for safety syntax
- AST construction tests
- Coverage calculation tests
- Metric calculation tests

### Integration Tests
- Full BCM example project
- Requirement traceability
- Metric rollup
- Documentation generation

### Validation
- Compare with manual safety analysis
- Check against ISO 26262 requirements
- Industry expert review

## Next Steps

1. Start with parser changes for `satisfies` keyword
2. Add minimal AST nodes for safety
3. Create basic safety analysis pass
4. Test with BCM example
5. Iterate based on findings