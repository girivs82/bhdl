//! Supplier and supply chain data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::ComponentId;

/// Supplier data for a component (aggregated from multiple suppliers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierData {
    pub component_id: ComponentId,
    pub suppliers: Vec<SupplierInfo>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Individual supplier information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierInfo {
    pub supplier_name: String,
    pub supplier_part_number: String,
    pub manufacturer_part_number: String,
    pub manufacturer: String,
    pub availability: i32,
    pub lead_time_days: Option<i32>,
    pub moq: i32,
    pub price_breaks: Vec<PriceBreak>,
    pub datasheet_url: Option<String>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Price break for quantity-based pricing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceBreak {
    pub quantity: i32,
    pub unit_price: f64,
    pub currency: String,
}

/// Component packaging types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackagingType {
    Reel,
    Tube,
    Tray,
    Bulk,
    CutTape,
    Other(String),
}

impl PackagingType {
    pub fn as_str(&self) -> &str {
        match self {
            PackagingType::Reel => "reel",
            PackagingType::Tube => "tube",
            PackagingType::Tray => "tray",
            PackagingType::Bulk => "bulk",
            PackagingType::CutTape => "cut_tape",
            PackagingType::Other(s) => s,
        }
    }
}

/// Supplier choice result for component selection
#[derive(Debug, Clone)]
pub struct SupplierChoice {
    pub supplier_info: SupplierInfo,
    pub unit_price: f64,
    pub total_price: f64,
    pub quantity_available: i32,
    pub lead_time_days: Option<i32>,
    pub score: f64, // Overall supplier score
}

impl SupplierChoice {
    /// Create a supplier choice for a given quantity
    pub fn new(supplier_info: SupplierInfo, quantity: i32) -> Self {
        let unit_price = Self::calculate_unit_price(&supplier_info.price_breaks, quantity);
        let total_price = unit_price * quantity as f64;
        let quantity_available = supplier_info.availability.min(quantity);
        let lead_time_days = supplier_info.lead_time_days;
        
        // Calculate score based on price, availability, and lead time
        let price_score = 100.0 / (1.0 + unit_price); // Lower price = higher score
        let availability_score = if supplier_info.availability >= quantity {
            100.0
        } else {
            (supplier_info.availability as f64 / quantity as f64) * 100.0
        };
        let lead_time_score = if let Some(lt) = lead_time_days {
            100.0 / (1.0 + lt as f64) // Shorter lead time = higher score
        } else {
            50.0 // Unknown lead time gets middle score
        };
        
        // Weighted scoring: 40% price, 40% availability, 20% lead time
        let score = price_score * 0.4 + availability_score * 0.4 + lead_time_score * 0.2;

        Self {
            supplier_info,
            unit_price,
            total_price,
            quantity_available,
            lead_time_days,
            score,
        }
    }

    /// Calculate unit price based on quantity and price breaks
    fn calculate_unit_price(price_breaks: &[PriceBreak], quantity: i32) -> f64 {
        // Find the best price break for the given quantity
        let mut best_price = f64::MAX;
        
        for price_break in price_breaks {
            if quantity >= price_break.quantity && price_break.unit_price < best_price {
                best_price = price_break.unit_price;
            }
        }
        
        best_price
    }
}

/// Summary of supplier data import operation
#[derive(Debug, Default)]
pub struct ImportSummary {
    pub updated_count: u32,
    pub added_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

impl ImportSummary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.error_count += 1;
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.updated_count + self.added_count + self.error_count;
        if total == 0 {
            0.0
        } else {
            (self.updated_count + self.added_count) as f64 / total as f64
        }
    }
}

impl SupplierData {
    /// Get the best supplier choices for a given quantity, ranked by score
    pub fn get_best_suppliers(&self, quantity: i32, max_results: usize) -> Vec<SupplierChoice> {
        let mut choices: Vec<SupplierChoice> = self.suppliers
            .iter()
            .map(|supplier| SupplierChoice::new(supplier.clone(), quantity))
            .collect();
        
        // Sort by score (highest first)
        choices.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        
        choices.into_iter().take(max_results).collect()
    }

    /// Check if any supplier has sufficient stock
    pub fn has_stock(&self, quantity: i32) -> bool {
        self.suppliers.iter().any(|s| s.availability >= quantity)
    }

    /// Get the best price available for a given quantity
    pub fn get_best_price(&self, quantity: i32) -> Option<f64> {
        self.suppliers
            .iter()
            .map(|supplier| SupplierChoice::calculate_unit_price(&supplier.price_breaks, quantity))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }
}