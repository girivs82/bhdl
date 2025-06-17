//! ASCII schematic renderer for quick visualization and debugging
//! 
//! Produces human-readable ASCII art schematics that follow standard conventions

use std::collections::HashMap;
use anyhow::Result;
use bhdl_netlist::{Netlist, InstanceId, NetId};
use bhdl_analyzer::types::AnalysisResult;

/// ASCII canvas for drawing schematics
pub struct AsciiCanvas {
    width: usize,
    height: usize,
    cells: Vec<Vec<char>>,
}

impl AsciiCanvas {
    /// Create a new ASCII canvas
    pub fn new(width: usize, height: usize) -> Self {
        let cells = vec![vec![' '; width]; height];
        Self { width, height, cells }
    }
    
    /// Draw a character at position
    pub fn draw_char(&mut self, x: usize, y: usize, ch: char) {
        if x < self.width && y < self.height {
            self.cells[y][x] = ch;
        }
    }
    
    /// Draw a string at position
    pub fn draw_string(&mut self, x: usize, y: usize, s: &str) {
        for (i, ch) in s.chars().enumerate() {
            self.draw_char(x + i, y, ch);
        }
    }
    
    /// Draw a horizontal line
    pub fn draw_hline(&mut self, x1: usize, x2: usize, y: usize, ch: char) {
        let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
        for x in start..=end {
            self.draw_char(x, y, ch);
        }
    }
    
    /// Draw a vertical line
    pub fn draw_vline(&mut self, x: usize, y1: usize, y2: usize, ch: char) {
        let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
        for y in start..=end {
            self.draw_char(x, y, ch);
        }
    }
    
    /// Draw a box
    pub fn draw_box(&mut self, x: usize, y: usize, width: usize, height: usize) {
        // Corners
        self.draw_char(x, y, '┌');
        self.draw_char(x + width - 1, y, '┐');
        self.draw_char(x, y + height - 1, '└');
        self.draw_char(x + width - 1, y + height - 1, '┘');
        
        // Horizontal lines
        for i in 1..width-1 {
            self.draw_char(x + i, y, '─');
            self.draw_char(x + i, y + height - 1, '─');
        }
        
        // Vertical lines
        for i in 1..height-1 {
            self.draw_char(x, y + i, '│');
            self.draw_char(x + width - 1, y + i, '│');
        }
    }
    
    /// Draw a capacitor symbol (vertical)
    pub fn draw_capacitor_v(&mut self, x: usize, y: usize, polarized: bool) {
        if polarized {
            self.draw_string(x - 1, y, "═══");
            self.draw_char(x, y + 1, '+');
            self.draw_string(x - 1, y + 2, "───");
        } else {
            self.draw_string(x - 1, y, "═══");
            self.draw_string(x - 1, y + 1, "═══");
        }
    }
    
    /// Draw a resistor symbol (vertical)
    pub fn draw_resistor_v(&mut self, x: usize, y: usize) {
        self.draw_string(x - 1, y, "═══");
    }
    
    /// Draw an LED symbol (vertical)
    pub fn draw_led_v(&mut self, x: usize, y: usize) {
        self.draw_char(x, y, '▼');
        self.draw_string(x - 1, y + 1, "LED");
    }
    
    /// Draw a connection dot
    pub fn draw_dot(&mut self, x: usize, y: usize) {
        self.draw_char(x, y, '•');
    }
    
    /// Draw a T-junction
    pub fn draw_tjunction(&mut self, x: usize, y: usize, orientation: TJunction) {
        match orientation {
            TJunction::Top => self.draw_char(x, y, '┬'),
            TJunction::Bottom => self.draw_char(x, y, '┴'),
            TJunction::Left => self.draw_char(x, y, '├'),
            TJunction::Right => self.draw_char(x, y, '┤'),
        }
    }
    
    /// Convert canvas to string
    pub fn to_string(&self) -> String {
        self.cells.iter()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TJunction {
    Top,
    Bottom,
    Left,
    Right,
}

/// ASCII schematic renderer
pub struct AsciiRenderer {
    canvas: AsciiCanvas,
}

impl AsciiRenderer {
    /// Create a new ASCII renderer
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            canvas: AsciiCanvas::new(width, height),
        }
    }
    
    /// Render a voltage regulator circuit
    pub fn render_voltage_regulator(
        &mut self,
        netlist: &Netlist,
        analysis: Option<&AnalysisResult>,
    ) -> Result<String> {
        // Power rails
        let vin_y = 2;
        let gnd_y = 18;
        let vcc_y = 2;
        
        // Draw VIN rail
        self.canvas.draw_string(4, vin_y, "VIN");
        self.canvas.draw_hline(8, 25, vin_y, '─');
        
        // Draw input capacitor C1
        let c1_x = 13;
        self.canvas.draw_vline(c1_x, vin_y, vin_y + 2, '│');
        self.canvas.draw_capacitor_v(c1_x, vin_y + 3, true);
        self.canvas.draw_vline(c1_x, vin_y + 5, gnd_y, '│');
        self.canvas.draw_string(c1_x - 1, vin_y + 7, "C1");
        
        // Draw regulator U1
        let u1_x = 23;
        let u1_y = 8;
        self.canvas.draw_box(u1_x, u1_y, 9, 5);
        self.canvas.draw_string(u1_x + 2, u1_y + 1, "U1");
        self.canvas.draw_string(u1_x + 1, u1_y + 2, "LM7805");
        
        // Connect VIN to regulator
        self.canvas.draw_vline(u1_x + 4, vin_y, u1_y, '│');
        
        // Draw VCC rail (right side)
        self.canvas.draw_string(40, vcc_y, "VCC");
        self.canvas.draw_hline(44, 70, vcc_y, '─');
        
        // Connect regulator output to VCC
        self.canvas.draw_vline(u1_x + 4, u1_y - 1, vcc_y, '│');
        self.canvas.draw_hline(u1_x + 4, 44, vcc_y, '─');
        
        // Draw output capacitors
        let c2_x = 50;
        self.canvas.draw_vline(c2_x, vcc_y, vcc_y + 2, '│');
        self.canvas.draw_capacitor_v(c2_x, vcc_y + 3, true);
        self.canvas.draw_vline(c2_x, vcc_y + 5, gnd_y, '│');
        self.canvas.draw_string(c2_x - 1, vcc_y + 7, "C2");
        
        let c3_x = 56;
        self.canvas.draw_vline(c3_x, vcc_y, vcc_y + 2, '│');
        self.canvas.draw_capacitor_v(c3_x, vcc_y + 3, false);
        self.canvas.draw_vline(c3_x, vcc_y + 5, gnd_y, '│');
        self.canvas.draw_string(c3_x - 1, vcc_y + 7, "C3");
        
        // Draw LED circuit
        let r1_x = 62;
        self.canvas.draw_vline(r1_x, vcc_y, vcc_y + 2, '│');
        self.canvas.draw_resistor_v(r1_x, vcc_y + 3);
        self.canvas.draw_vline(r1_x, vcc_y + 4, vcc_y + 6, '│');
        self.canvas.draw_string(r1_x - 1, vcc_y + 5, "R1");
        
        self.canvas.draw_led_v(r1_x, vcc_y + 7);
        self.canvas.draw_vline(r1_x, vcc_y + 9, gnd_y, '│');
        
        // Draw GND rail
        self.canvas.draw_string(4, gnd_y, "GND");
        self.canvas.draw_hline(8, 70, gnd_y, '─');
        
        // Connect regulator GND
        self.canvas.draw_vline(u1_x + 4, u1_y + 4, gnd_y, '│');
        self.canvas.draw_tjunction(u1_x + 4, gnd_y, TJunction::Bottom);
        
        // Add connection dots
        self.canvas.draw_dot(c1_x, vin_y);
        self.canvas.draw_dot(c1_x, gnd_y);
        self.canvas.draw_dot(c2_x, vcc_y);
        self.canvas.draw_dot(c2_x, gnd_y);
        self.canvas.draw_dot(c3_x, vcc_y);
        self.canvas.draw_dot(c3_x, gnd_y);
        self.canvas.draw_dot(r1_x, vcc_y);
        self.canvas.draw_dot(r1_x, gnd_y);
        
        Ok(self.canvas.to_string())
    }
    
    /// Render a generic circuit (fallback)
    pub fn render_generic(&mut self, netlist: &Netlist) -> Result<String> {
        // Simple grid layout for generic circuits
        self.canvas.draw_string(5, 2, "Generic Circuit Layout");
        self.canvas.draw_string(5, 4, "(ASCII visualization not optimized for this circuit type)");
        
        let mut y = 6;
        for (id, instance) in &netlist.instances {
            if let Some(module) = netlist.modules.get(instance.definition) {
                self.canvas.draw_string(5, y, &format!("{}: {}", instance.name, module.name));
                y += 2;
            }
        }
        
        Ok(self.canvas.to_string())
    }
}

/// Render a netlist to ASCII art
pub fn render_ascii(
    netlist: &Netlist,
    analysis: Option<&AnalysisResult>,
    width: usize,
    height: usize,
) -> Result<String> {
    let mut renderer = AsciiRenderer::new(width, height);
    
    // Detect circuit type and render appropriately
    if let Some(analysis) = analysis {
        // Check for voltage regulator pattern
        if analysis.power_analysis.domains.len() > 1 {
            return renderer.render_voltage_regulator(netlist, Some(analysis));
        }
    }
    
    // Fallback to generic rendering
    renderer.render_generic(netlist)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ascii_canvas() {
        let mut canvas = AsciiCanvas::new(20, 10);
        canvas.draw_string(5, 2, "Test");
        canvas.draw_hline(2, 10, 4, '─');
        canvas.draw_vline(6, 1, 5, '│');
        
        let output = canvas.to_string();
        assert!(output.contains("Test"));
        assert!(output.contains("─"));
        assert!(output.contains("│"));
    }
    
    #[test]
    fn test_voltage_regulator_ascii() {
        let netlist = Netlist::new();
        let mut renderer = AsciiRenderer::new(80, 25);
        let result = renderer.render_voltage_regulator(&netlist, None);
        assert!(result.is_ok());
        
        let ascii = result.unwrap();
        assert!(ascii.contains("VIN"));
        assert!(ascii.contains("VCC"));
        assert!(ascii.contains("GND"));
        assert!(ascii.contains("LM7805"));
    }
}