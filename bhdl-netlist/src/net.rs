// Contains the Net struct
use crate::types::{ConnectionPoint, NetClass};
use serde::{Serialize, Deserialize};
use std::vec::Vec;

// Represents a wire connecting ports/pins
#[derive(Debug, Serialize, Deserialize)]
pub struct Net {
    pub name: Option<String>, // Optional net name
    pub connections: Vec<ConnectionPoint>,
    pub net_class: NetClass, // Classification for routing and constraints
    // Add type, width, drive strength, etc. later
} 