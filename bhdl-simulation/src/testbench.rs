// Testbench module - minimal stub for compilation

use crate::Result;

#[derive(Debug, Clone)]
pub struct Testbench {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
}

#[derive(Debug, Clone, Copy)]
pub enum CaptureMode {
    All,
    Selected,
}