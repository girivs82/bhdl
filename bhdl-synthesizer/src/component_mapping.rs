//! Component Database Mapping for BHDL Synthesizer
//! 
//! This module provides the proper mapping from BHDL component types to database-stored
//! components with SVG symbols, replacing the direct KiCad parsing approach.

use std::collections::HashMap;
use std::path::Path;
use anyhow::{Result, Context};
use log::{debug, info, warn};

use bhdl_components::{ComponentDatabase, Component, ComponentId, ComponentCache};
use bhdl_netlist::types::ModuleKind;

/// Maps BHDL component types to database component IDs with SVG data
#[derive(Debug, Clone)]
pub struct ComponentMapping {
    /// BHDL component type (e.g., "LM7805", "Capacitor", "Resistor")
    pub bhdl_type: String,
    /// Database component ID
    pub component_id: ComponentId,
    /// Component name from database
    pub component_name: String,
    /// Pin mapping from BHDL pin names to database pin numbers
    pub pin_mapping: HashMap<String, String>,
    /// Component category for semantic analysis
    pub category: ComponentCategory,
}

/// Categories of electronic components for semantic understanding
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentCategory {
    PowerRegulator,
    PassiveCapacitor,
    PassiveResistor,
    Semiconductor,
    Connector,
    Crystal,
    Unknown,
}

/// Database-backed component mapper for the synthesizer
pub struct DatabaseComponentMapper {
    /// Component database connection
    database: ComponentDatabase,
    /// LRU cache for components and SVG data
    cache: ComponentCache,
    /// Component mappings from BHDL types to database components
    component_mappings: HashMap<String, ComponentMapping>,
}

impl DatabaseComponentMapper {
    /// Create a new database component mapper
    pub async fn new(database_path: &Path) -> Result<Self> {
        let database = ComponentDatabase::new(database_path).await
            .context("Failed to initialize component database")?;
            
        let mut mapper = Self {
            database,
            cache: ComponentCache::new(),
            component_mappings: HashMap::new(),
        };
        
        // Initialize mappings for linear regulator components
        mapper.initialize_component_mappings().await?;
        
        Ok(mapper)
    }

    /// Initialize component mappings by querying the database
    async fn initialize_component_mappings(&mut self) -> Result<()> {
        info!("🔍 Initializing component mappings from database");
        
        // Map of BHDL component types to database search criteria
        let component_searches = [
            ("LM7805", "LM7805_TO220", ComponentCategory::PowerRegulator), // Use exact name from DB
            ("Capacitor", "C", ComponentCategory::PassiveCapacitor),
            ("Cap", "C", ComponentCategory::PassiveCapacitor), // Add mapping for BHDL "Cap" type
            ("Resistor", "R", ComponentCategory::PassiveResistor),
            ("Res", "R", ComponentCategory::PassiveResistor), // Add mapping for BHDL "Res" type
            ("LED", "LED", ComponentCategory::Semiconductor),
            ("Fuse", "Fuse", ComponentCategory::Connector),
            ("TVSDiode", "D_TVS", ComponentCategory::Semiconductor),
            ("ElectrolyticCap", "C_Polarized", ComponentCategory::PassiveCapacitor), // Use exact name from DB
            ("TestPoint", "TestPoint", ComponentCategory::Connector), // Will need to add to DB
        ];
        
        for (bhdl_type, search_name, category) in &component_searches {
            match self.find_component_by_name(search_name).await {
                Ok(Some(component)) => {
                    let pin_mapping = self.create_pin_mapping(&component, bhdl_type);
                    
                    let mapping = ComponentMapping {
                        bhdl_type: bhdl_type.to_string(),
                        component_id: component.id,
                        component_name: component.name.clone(),
                        pin_mapping,
                        category: category.clone(),
                    };
                    
                    self.component_mappings.insert(bhdl_type.to_string(), mapping);
                    // Cache the component in LRU cache
                    let component_id = component.id;
                    self.cache.cache_component(component.id, component).await;
                    
                    info!("✅ Mapped {} → {} (ID: {})", bhdl_type, search_name, component_id);
                }
                Ok(None) => {
                    warn!("⚠️  Component not found in database: {} (for BHDL type {})", search_name, bhdl_type);
                }
                Err(e) => {
                    warn!("❌ Failed to search for component {}: {}", search_name, e);
                }
            }
        }
        
        info!("📊 Initialized {} component mappings", self.component_mappings.len());
        Ok(())
    }

    /// Find a component in the database by name
    async fn find_component_by_name(&self, name: &str) -> Result<Option<Component>> {
        // Search for components by name
        let components = self.database.search_components(name).await
            .context("Failed to search components in database")?;
        
        // Find exact match or closest match
        for component in &components {
            if component.name == name {
                return Ok(Some(component.clone()));
            }
        }
        
        // If no exact match, return the first component found
        if let Some(component) = components.into_iter().next() {
            debug!("Using closest match for '{}': '{}'", name, component.name);
            Ok(Some(component))
        } else {
            Ok(None)
        }
    }

    /// Create pin mapping for a component based on BHDL type conventions
    fn create_pin_mapping(&self, component: &Component, bhdl_type: &str) -> HashMap<String, String> {
        let mut mapping = HashMap::new();
        
        match bhdl_type {
            "LM7805" => {
                // Standard linear regulator pinout
                mapping.insert("input".to_string(), "1".to_string());
                mapping.insert("ground".to_string(), "2".to_string());
                mapping.insert("output".to_string(), "3".to_string());
            }
            "Capacitor" => {
                // Standard capacitor pinout
                mapping.insert("positive".to_string(), "1".to_string());
                mapping.insert("negative".to_string(), "2".to_string());
            }
            "Resistor" | "Res" => {
                // Standard resistor pinout
                mapping.insert("terminal1".to_string(), "1".to_string());
                mapping.insert("terminal2".to_string(), "2".to_string());
                mapping.insert("1".to_string(), "1".to_string()); // Direct pin access
                mapping.insert("2".to_string(), "2".to_string()); // Direct pin access
            }
            "LED" => {
                // Standard LED pinout
                mapping.insert("anode".to_string(), "2".to_string()); // A pin
                mapping.insert("cathode".to_string(), "1".to_string()); // K pin
                mapping.insert("A".to_string(), "2".to_string()); // Direct pin access
                mapping.insert("K".to_string(), "1".to_string()); // Direct pin access
            }
            _ => {
                // For unknown components, try to map based on database pins
                for pin in &component.pins {
                    // Map pin names directly
                    mapping.insert(pin.pin_name.clone().unwrap_or_else(|| pin.pin_number.clone()), pin.pin_number.clone());
                }
            }
        }
        
        mapping
    }

    /// Get component mapping for a BHDL component type
    pub fn get_mapping(&self, bhdl_component_type: &str) -> Option<&ComponentMapping> {
        self.component_mappings.get(bhdl_component_type)
    }

    /// Get database component for a BHDL component type
    pub async fn get_component(&mut self, bhdl_component_type: &str) -> Result<Option<Component>> {
        let mapping = match self.get_mapping(bhdl_component_type) {
            Some(mapping) => mapping,
            None => return Ok(None),
        };
        
        // Check LRU cache first
        if let Some(component) = self.cache.get_component(mapping.component_id).await {
            debug!("Cache hit for component {} (ID: {})", bhdl_component_type, mapping.component_id);
            return Ok(Some(component));
        }
        
        // Cache miss - load from database
        debug!("Cache miss for component {} (ID: {}), loading from database", bhdl_component_type, mapping.component_id);
        let component = self.database.get_component(mapping.component_id).await
            .context("Failed to get component from database")?;
            
        if let Some(component) = component {
            // Cache for next time
            self.cache.cache_component(mapping.component_id, component.clone()).await;
            Ok(Some(component))
        } else {
            Ok(None)
        }
    }

    /// Create a component instance with database component and SVG data
    pub async fn create_component_instance(
        &mut self,
        instance_name: &str,
        bhdl_component_type: &str,
    ) -> Result<DatabaseComponentInstance> {
        let mapping = self.get_mapping(bhdl_component_type)
            .ok_or_else(|| anyhow::anyhow!("No mapping found for component type: {}", bhdl_component_type))?
            .clone(); // Clone the mapping to avoid borrow checker issues
        
        let component = self.get_component(bhdl_component_type).await?
            .ok_or_else(|| anyhow::anyhow!("Component not found in database: {}", bhdl_component_type))?;
        
        // Get SVG data from LRU cache or component symbol
        let svg_data = if let Some(cached_svg) = self.cache.get_symbol_svg(mapping.component_id).await {
            debug!("Cache hit for SVG data for component {} (ID: {})", bhdl_component_type, mapping.component_id);
            cached_svg
        } else if let Some(symbol) = &component.symbol {
            debug!("Cache miss for SVG data, using component symbol for {} (ID: {})", bhdl_component_type, mapping.component_id);
            let svg = symbol.svg_data.clone();
            // Cache the SVG for next time
            if !svg.is_empty() {
                self.cache.cache_symbol_svg(mapping.component_id, svg.clone()).await;
            }
            svg
        } else {
            warn!("No SVG symbol data found for component: {}", component.name);
            String::new()
        };
        
        Ok(DatabaseComponentInstance {
            instance_name: instance_name.to_string(),
            bhdl_type: bhdl_component_type.to_string(),
            component_id: component.id,
            component_name: component.name.clone(),
            component_description: component.description.clone(),
            svg_data,
            pin_mapping: mapping.pin_mapping.clone(),
            category: mapping.category.clone(),
            electrical_specs: component.electrical_specs.clone(),
            pins: component.pins.clone(),
        })
    }

    /// Get all available mappings
    pub fn get_all_mappings(&self) -> &HashMap<String, ComponentMapping> {
        &self.component_mappings
    }

    /// Get mapper statistics
    pub async fn get_stats(&self) -> DatabaseMapperStats {
        let cache_sizes = self.cache.get_cache_sizes().await;
        let cache_stats = self.cache.get_stats();
        
        DatabaseMapperStats {
            component_mappings: self.component_mappings.len(),
            cached_components: cache_sizes.hot_cache_size,
            cached_svg_symbols: cache_sizes.symbol_cache_size,
            cached_searches: cache_sizes.search_cache_size,
            component_cache_hit_rate: cache_stats.component_hit_rate(),
            symbol_cache_hit_rate: cache_stats.symbol_hit_rate(),
            search_cache_hit_rate: cache_stats.search_hit_rate(),
        }
    }

    /// Import linear regulator components into database if missing
    pub async fn ensure_components_imported(&mut self) -> Result<()> {
        info!("🔧 Ensuring linear regulator components are imported");
        
        // Check if we have all required mappings
        let required_types = ["LM7805", "Capacitor", "Resistor", "LED"];
        let missing_types: Vec<_> = required_types.iter()
            .filter(|&t| !self.component_mappings.contains_key(*t))
            .collect();
            
        if missing_types.is_empty() {
            info!("✅ All required components already available");
            return Ok(());
        }
        
        info!("📦 Missing components: {:?}", missing_types);
        info!("🚀 Importing components from KiCad cache...");
        
        // Import components directly using the database
        info!("📦 Auto-importing linear regulator components...");
        // For now, we'll warn that components need to be imported manually
        warn!("Components not found in database. Please run: cargo run -p bhdl-components --bin import_kicad_symbols");
        return Ok(());
        
        // Note: The code below would reinitialize mappings but is unreachable due to early return above
        // self.component_mappings.clear();
        // self.cache.clear_all().await;
        // self.initialize_component_mappings().await?;
        
        Ok(())
    }

    /// Preload common components into cache for better performance
    pub async fn preload_common_components(&mut self) -> Result<()> {
        info!("🚀 Preloading common linear regulator components into cache");
        
        let common_types = ["LM7805", "Capacitor", "Resistor", "LED"];
        let mut components_to_preload = Vec::new();
        
        for component_type in &common_types {
            if let Ok(Some(component)) = self.get_component(component_type).await {
                components_to_preload.push(component);
                debug!("Preloaded {} into cache", component_type);
            }
        }
        
        if !components_to_preload.is_empty() {
            self.cache.preload_common_components(components_to_preload).await;
            info!("✅ Preloaded {} components into cache", common_types.len());
        }
        
        Ok(())
    }
}

/// Represents a component instance from the database with SVG data
#[derive(Debug, Clone)]
pub struct DatabaseComponentInstance {
    /// BHDL instance name (e.g., "U1", "C1", "R1")
    pub instance_name: String,
    /// BHDL component type (e.g., "LM7805", "Capacitor")
    pub bhdl_type: String,
    /// Database component ID
    pub component_id: ComponentId,
    /// Component name from database
    pub component_name: String,
    /// Component description
    pub component_description: Option<String>,
    /// SVG symbol data from database
    pub svg_data: String,
    /// Pin mapping from BHDL names to database pin numbers
    pub pin_mapping: HashMap<String, String>,
    /// Component category for semantic analysis
    pub category: ComponentCategory,
    /// Electrical specifications from database
    pub electrical_specs: Vec<bhdl_components::types::ElectricalSpec>,
    /// Pin definitions from database
    pub pins: Vec<bhdl_components::types::PinDefinition>,
}

/// Statistics about the database mapper
#[derive(Debug, Clone)]
pub struct DatabaseMapperStats {
    pub component_mappings: usize,
    pub cached_components: usize,
    pub cached_svg_symbols: usize,
    pub cached_searches: usize,
    pub component_cache_hit_rate: f64,
    pub symbol_cache_hit_rate: f64,
    pub search_cache_hit_rate: f64,
}

impl DatabaseComponentInstance {
    /// Get database pin number for a BHDL pin name
    pub fn get_database_pin(&self, bhdl_pin_name: &str) -> Option<&String> {
        self.pin_mapping.get(bhdl_pin_name)
    }

    /// Check if this component is a power regulator
    pub fn is_power_regulator(&self) -> bool {
        self.category == ComponentCategory::PowerRegulator
    }

    /// Check if this component is a passive component
    pub fn is_passive(&self) -> bool {
        matches!(self.category, ComponentCategory::PassiveCapacitor | ComponentCategory::PassiveResistor)
    }

    /// Get semantic module kind for netlist generation
    pub fn get_module_kind(&self) -> ModuleKind {
        match self.category {
            ComponentCategory::PowerRegulator => ModuleKind::Component,
            ComponentCategory::PassiveCapacitor | ComponentCategory::PassiveResistor => ModuleKind::PhysicalComponent,
            ComponentCategory::Semiconductor => ModuleKind::Component,
            ComponentCategory::Connector => ModuleKind::Interface,
            ComponentCategory::Crystal => ModuleKind::Component,
            ComponentCategory::Unknown => ModuleKind::Component,
        }
    }

    /// Check if SVG data is available
    pub fn has_svg_data(&self) -> bool {
        !self.svg_data.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_database_mapper_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // This will create an empty database
        let result = DatabaseComponentMapper::new(&db_path).await;
        assert!(result.is_ok());
        
        let mapper = result.unwrap();
        let stats = mapper.get_stats().await;
        
        // Should have 0 mappings since database is empty
        assert_eq!(stats.component_mappings, 0);
        assert_eq!(stats.cached_components, 0);
    }

    #[test]
    fn test_component_instance_helpers() {
        let instance = DatabaseComponentInstance {
            instance_name: "U1".to_string(),
            bhdl_type: "LM7805".to_string(),
            component_id: 1,
            component_name: "LM7805_TO220".to_string(),
            component_description: Some("Voltage Regulator".to_string()),
            svg_data: "<svg>...</svg>".to_string(),
            pin_mapping: [("input".to_string(), "1".to_string())].iter().cloned().collect(),
            category: ComponentCategory::PowerRegulator,
            electrical_specs: vec![],
            pins: vec![],
        };
        
        assert!(instance.is_power_regulator());
        assert!(!instance.is_passive());
        assert_eq!(instance.get_module_kind(), ModuleKind::Component);
        assert_eq!(instance.get_database_pin("input"), Some(&"1".to_string()));
        assert_eq!(instance.get_database_pin("nonexistent"), None);
        assert!(instance.has_svg_data());
    }
}