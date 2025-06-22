//! Interactive debugger for simulation control

use std::collections::VecDeque;
use crate::error::{SimulationResult, SimulationError};
use super::{
    breakpoint::{BreakpointManager, Breakpoint, BreakpointType, BreakpointCondition},
    watchpoint::{WatchpointManager, Watchpoint, WatchpointType, WatchpointTrigger},
    inspector::{StateInspector, format_inspection_result},
};

/// Debug commands
#[derive(Debug, Clone)]
pub enum DebugCommand {
    /// Continue execution
    Continue,
    /// Step one time unit
    Step,
    /// Step N time units
    StepN(u32),
    /// Step to specific time
    StepTo(f64),
    /// Run until next event
    Next,
    /// Set breakpoint
    Break(BreakpointType, Option<BreakpointCondition>),
    /// Remove breakpoint
    Delete(u32),
    /// List breakpoints
    List,
    /// Enable/disable breakpoint
    Enable(u32, bool),
    /// Set watchpoint
    Watch(WatchpointType, WatchpointTrigger),
    /// Remove watchpoint
    Unwatch(u32),
    /// Inspect state
    Inspect(String),
    /// Print backtrace
    Backtrace,
    /// Show current time
    Time,
    /// Show statistics
    Stats,
    /// Quit debugger
    Quit,
}

/// Debugger state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DebugState {
    /// Running normally
    Running,
    /// Paused at breakpoint
    Paused,
    /// Stepping through simulation
    Stepping,
    /// Stopped/terminated
    Stopped,
}

/// Statistics tracked by debugger
#[derive(Debug, Default)]
pub struct DebugStatistics {
    pub total_steps: u64,
    pub breakpoint_hits: u64,
    pub watchpoint_triggers: u64,
    pub commands_executed: u64,
    pub time_elapsed: f64,
}

/// Interactive debugger for simulations
pub struct Debugger {
    /// Current debugger state
    state: DebugState,
    /// Breakpoint manager
    breakpoints: BreakpointManager,
    /// Watchpoint manager
    watchpoints: WatchpointManager,
    /// Command history
    history: VecDeque<DebugCommand>,
    /// Maximum history size
    max_history: usize,
    /// Debug statistics
    stats: DebugStatistics,
    /// Current simulation time
    current_time: f64,
    /// Execution trace for backtrace
    trace: Vec<String>,
    /// Maximum trace depth
    max_trace_depth: usize,
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            state: DebugState::Running,
            breakpoints: BreakpointManager::new(),
            watchpoints: WatchpointManager::new(),
            history: VecDeque::new(),
            max_history: 100,
            stats: DebugStatistics::default(),
            current_time: 0.0,
            trace: Vec::new(),
            max_trace_depth: 50,
        }
    }

    pub fn execute_command(&mut self, cmd: DebugCommand) -> SimulationResult<()> {
        self.history.push_back(cmd.clone());
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }
        self.stats.commands_executed += 1;

        match cmd {
            DebugCommand::Continue => {
                self.state = DebugState::Running;
                Ok(())
            }
            DebugCommand::Step => {
                self.state = DebugState::Stepping;
                self.stats.total_steps += 1;
                Ok(())
            }
            DebugCommand::StepN(n) => {
                self.state = DebugState::Stepping;
                self.stats.total_steps += n as u64;
                Ok(())
            }
            DebugCommand::StepTo(_time) => {
                self.state = DebugState::Running;
                Ok(())
            }
            DebugCommand::Next => {
                self.state = DebugState::Stepping;
                Ok(())
            }
            DebugCommand::Break(bp_type, condition) => {
                let bp = Breakpoint {
                    id: 0,
                    bp_type,
                    condition: condition.unwrap_or(BreakpointCondition::Always),
                    enabled: true,
                    hit_count: 0,
                    label: None,
                    one_shot: false,
                };
                let id = self.breakpoints.add_breakpoint(bp);
                println!("Breakpoint {} set", id);
                Ok(())
            }
            DebugCommand::Delete(id) => {
                self.breakpoints.remove_breakpoint(id)?;
                println!("Breakpoint {} removed", id);
                Ok(())
            }
            DebugCommand::List => {
                let bps = self.breakpoints.get_all_breakpoints();
                if bps.is_empty() {
                    println!("No breakpoints set");
                } else {
                    println!("Breakpoints:");
                    for bp in bps {
                        println!("  {} {} - {:?} ({} hits)", 
                            bp.id,
                            if bp.enabled { "●" } else { "○" },
                            bp.bp_type,
                            bp.hit_count
                        );
                    }
                }
                Ok(())
            }
            DebugCommand::Enable(id, enabled) => {
                self.breakpoints.enable_breakpoint(id, enabled)?;
                println!("Breakpoint {} {}", id, if enabled { "enabled" } else { "disabled" });
                Ok(())
            }
            DebugCommand::Watch(wp_type, trigger) => {
                let wp = Watchpoint {
                    id: 0,
                    wp_type,
                    trigger,
                    enabled: true,
                    last_value: None,
                    trigger_count: 0,
                    label: None,
                    log_changes: true,
                };
                let id = self.watchpoints.add_watchpoint(wp);
                println!("Watchpoint {} set", id);
                Ok(())
            }
            DebugCommand::Unwatch(id) => {
                self.watchpoints.remove_watchpoint(id)?;
                println!("Watchpoint {} removed", id);
                Ok(())
            }
            DebugCommand::Time => {
                println!("Current time: {:.9}s", self.current_time);
                Ok(())
            }
            DebugCommand::Stats => {
                println!("Debug Statistics:");
                println!("  Total steps: {}", self.stats.total_steps);
                println!("  Breakpoint hits: {}", self.stats.breakpoint_hits);
                println!("  Watchpoint triggers: {}", self.stats.watchpoint_triggers);
                println!("  Commands executed: {}", self.stats.commands_executed);
                println!("  Time elapsed: {:.9}s", self.stats.time_elapsed);
                Ok(())
            }
            DebugCommand::Backtrace => {
                if self.trace.is_empty() {
                    println!("No backtrace available");
                } else {
                    println!("Backtrace:");
                    for (i, entry) in self.trace.iter().enumerate() {
                        println!("  #{}: {}", i, entry);
                    }
                }
                Ok(())
            }
            DebugCommand::Quit => {
                self.state = DebugState::Stopped;
                Ok(())
            }
            _ => Ok(()), // Handled elsewhere
        }
    }

    pub fn handle_inspection(&self, path: &str, inspector: &StateInspector) -> SimulationResult<()> {
        let result = inspector.inspect(path);
        let formatted = format_inspection_result(&result, 0);
        println!("{}", formatted);
        Ok(())
    }

    pub fn check_breakpoints(&mut self, time: f64) -> Option<&Breakpoint> {
        if let Some(bp) = self.breakpoints.check_time_breakpoint(time) {
            self.state = DebugState::Paused;
            self.stats.breakpoint_hits += 1;
            println!("Breakpoint {} hit at time {:.9}s", bp.id, time);
            return Some(bp);
        }
        None
    }

    pub fn check_instance_breakpoint(&mut self, instance_id: bhdl_netlist::InstanceId) -> Option<&Breakpoint> {
        if let Some(bp) = self.breakpoints.check_instance_breakpoint(instance_id) {
            self.state = DebugState::Paused;
            self.stats.breakpoint_hits += 1;
            println!("Breakpoint {} hit at instance {:?}", bp.id, instance_id);
            return Some(bp);
        }
        None
    }

    pub fn update_time(&mut self, time: f64) {
        self.current_time = time;
        self.stats.time_elapsed = time;
    }

    pub fn add_trace(&mut self, entry: String) {
        self.trace.push(entry);
        if self.trace.len() > self.max_trace_depth {
            self.trace.remove(0);
        }
    }

    pub fn get_state(&self) -> DebugState {
        self.state
    }

    pub fn set_state(&mut self, state: DebugState) {
        self.state = state;
    }

    pub fn is_paused(&self) -> bool {
        self.state == DebugState::Paused
    }

    pub fn should_step(&self) -> bool {
        self.state == DebugState::Stepping
    }

    pub fn get_breakpoints(&mut self) -> &mut BreakpointManager {
        &mut self.breakpoints
    }

    pub fn get_watchpoints(&mut self) -> &mut WatchpointManager {
        &mut self.watchpoints
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn get_last_command(&self) -> Option<&DebugCommand> {
        self.history.back()
    }
}

/// Parse debug command from string
pub fn parse_debug_command(input: &str) -> Result<DebugCommand, String> {
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    
    if parts.is_empty() {
        return Err("Empty command".to_string());
    }

    match parts[0] {
        "c" | "continue" => Ok(DebugCommand::Continue),
        "s" | "step" => {
            if parts.len() > 1 {
                if let Ok(n) = parts[1].parse::<u32>() {
                    Ok(DebugCommand::StepN(n))
                } else {
                    Err("Invalid step count".to_string())
                }
            } else {
                Ok(DebugCommand::Step)
            }
        }
        "n" | "next" => Ok(DebugCommand::Next),
        "b" | "break" => {
            if parts.len() < 2 {
                return Err("Break requires target".to_string());
            }
            
            let bp_type = match parts[1] {
                "time" => {
                    if parts.len() < 3 {
                        return Err("Time breakpoint requires time value".to_string());
                    }
                    let time = parts[2].parse::<f64>()
                        .map_err(|_| "Invalid time value".to_string())?;
                    BreakpointType::Time(time)
                }
                _ => return Err("Unknown breakpoint type".to_string()),
            };
            
            Ok(DebugCommand::Break(bp_type, None))
        }
        "d" | "delete" => {
            if parts.len() < 2 {
                return Err("Delete requires breakpoint ID".to_string());
            }
            let id = parts[1].parse::<u32>()
                .map_err(|_| "Invalid breakpoint ID".to_string())?;
            Ok(DebugCommand::Delete(id))
        }
        "l" | "list" => Ok(DebugCommand::List),
        "i" | "inspect" => {
            if parts.len() < 2 {
                return Err("Inspect requires path".to_string());
            }
            Ok(DebugCommand::Inspect(parts[1..].join(".")))
        }
        "t" | "time" => Ok(DebugCommand::Time),
        "stats" => Ok(DebugCommand::Stats),
        "bt" | "backtrace" => Ok(DebugCommand::Backtrace),
        "q" | "quit" => Ok(DebugCommand::Quit),
        _ => Err(format!("Unknown command: {}", parts[0])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing() {
        assert!(matches!(parse_debug_command("continue").unwrap(), DebugCommand::Continue));
        assert!(matches!(parse_debug_command("c").unwrap(), DebugCommand::Continue));
        assert!(matches!(parse_debug_command("step").unwrap(), DebugCommand::Step));
        assert!(matches!(parse_debug_command("step 5").unwrap(), DebugCommand::StepN(5)));
        assert!(matches!(parse_debug_command("break time 1e-9").unwrap(), 
            DebugCommand::Break(BreakpointType::Time(_), None)));
        assert!(matches!(parse_debug_command("delete 1").unwrap(), DebugCommand::Delete(1)));
        assert!(matches!(parse_debug_command("inspect cpu.pins").unwrap(), 
            DebugCommand::Inspect(s) if s == "cpu.pins"));
    }

    #[test]
    fn test_debugger_state_transitions() {
        let mut debugger = Debugger::new();
        
        assert_eq!(debugger.get_state(), DebugState::Running);
        
        debugger.execute_command(DebugCommand::Step).unwrap();
        assert_eq!(debugger.get_state(), DebugState::Stepping);
        
        debugger.execute_command(DebugCommand::Continue).unwrap();
        assert_eq!(debugger.get_state(), DebugState::Running);
        
        debugger.execute_command(DebugCommand::Quit).unwrap();
        assert_eq!(debugger.get_state(), DebugState::Stopped);
    }

    #[test]
    fn test_command_history() {
        let mut debugger = Debugger::new();
        debugger.max_history = 3;
        
        debugger.execute_command(DebugCommand::Step).unwrap();
        debugger.execute_command(DebugCommand::Continue).unwrap();
        debugger.execute_command(DebugCommand::Time).unwrap();
        debugger.execute_command(DebugCommand::Stats).unwrap();
        
        // Should only keep last 3 commands
        assert_eq!(debugger.history.len(), 3);
        assert!(matches!(debugger.get_last_command(), Some(DebugCommand::Stats)));
    }
}