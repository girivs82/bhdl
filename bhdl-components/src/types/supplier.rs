//! Supplier and supply chain data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::ComponentId;

/// Supplier data for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierData {
    pub id: u32,
    pub component_id: ComponentId,
    pub supplier_name: String,
    pub part_number: String,
    pub manufacturer_part_number: String,
    pub price_breaks: Vec<PriceBreak>,
    pub stock_quantity: u32,
    pub lead_time_days: u32,
    pub minimum_order_quantity: u32,
    pub packaging: PackagingType,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub currency: String,
}

/// Price break for quantity-based pricing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceBreak {
    pub quantity: u32,
    pub unit_price: f64,
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
    pub supplier_data: SupplierData,
    pub unit_price: f64,
    pub total_price: f64,
    pub quantity_available: u32,
    pub lead_time_days: u32,
    pub score: f64, // Overall supplier score
}

impl SupplierChoice {
    /// Create a supplier choice for a given quantity
    pub fn new(supplier_data: SupplierData, quantity: u32) -> Self {
        let unit_price = Self::calculate_unit_price(&supplier_data.price_breaks, quantity);
        let total_price = unit_price * quantity as f64;
        let quantity_available = supplier_data.stock_quantity.min(quantity);
        let lead_time_days = supplier_data.lead_time_days;
        
        // Calculate score based on price, availability, and lead time
        let price_score = 100.0 / (1.0 + unit_price); // Lower price = higher score
        let availability_score = if supplier_data.stock_quantity >= quantity {
            100.0
        } else {
            (supplier_data.stock_quantity as f64 / quantity as f64) * 100.0
        };
        let lead_time_score = 100.0 / (1.0 + lead_time_days as f64); // Shorter lead time = higher score
        
        // Weighted scoring: 40% price, 40% availability, 20% lead time
        let score = price_score * 0.4 + availability_score * 0.4 + lead_time_score * 0.2;

        Self {
            supplier_data,
            unit_price,
            total_price,
            quantity_available,
            lead_time_days,
            score,
        }
    }

    /// Calculate unit price based on quantity and price breaks
    fn calculate_unit_price(price_breaks: &[PriceBreak], quantity: u32) -> f64 {
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
    /// Get the best price for a given quantity
    pub fn get_price_for_quantity(&self, quantity: u32) -> f64 {
        SupplierChoice::calculate_unit_price(&self.price_breaks, quantity)
    }

    /// Check if supplier has sufficient stock
    pub fn has_stock(&self, quantity: u32) -> bool {
        self.stock_quantity >= quantity
    }

    /// Get effective lead time (accounting for stock availability)
    pub fn effective_lead_time(&self, quantity: u32) -> u32 {
        if self.has_stock(quantity) {
            self.lead_time_days
        } else {
            // If insufficient stock, assume longer lead time
            self.lead_time_days + 14 // Add 2 weeks for restocking
        }
    }
}