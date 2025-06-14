//! Component synthesis module
//!
//! This module provides comprehensive component synthesis capabilities, converting
//! abstract ComponentRequirements into concrete ComponentOption selections through
//! intelligent matching, optimization, and supplier integration.

pub mod engine;
pub mod matcher;
pub mod optimizer;
pub mod synthesizer;
pub mod selector;

// Re-export the main synthesis engine
pub use engine::SynthesisEngine;

use crate::types::*;
use crate::database::ComponentDatabase;

/// Main component synthesizer (legacy interface - use SynthesisEngine for new code)
pub struct ComponentSynthesizer {
    engine: SynthesisEngine,
}

impl ComponentSynthesizer {
    pub fn new() -> Self {
        Self {
            engine: SynthesisEngine::new(),
        }
    }
    
    pub async fn synthesize(
        &self,
        component_type: &str,
        requirements: &ComponentRequirements,
        database: &ComponentDatabase,
    ) -> anyhow::Result<SynthesisResult> {
        // Delegate to the new synthesis engine
        self.engine.synthesize_component(component_type, requirements, database).await
    }
}

impl Default for ComponentSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}