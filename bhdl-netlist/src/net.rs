// Contains the Net struct
use crate::types::ConnectionPoint;
use serde::{Serialize, Deserialize};
use std::vec::Vec;

// Represents a wire connecting ports/pins
#[derive(Debug, Serialize, Deserialize)]
pub struct Net {
    pub name: Option<String>, // Optional net name
    pub connections: Vec<ConnectionPoint>,
    // Add type, width, drive strength, etc. later
} 