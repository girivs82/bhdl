//! Control interface for simulation

use crate::error::{Breakpoint, BreakpointId, SimulationError, SimulationResult};
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use super::state::{StateMachine, Event};

/// Commands that can be sent to the simulation
#[derive(Debug, Clone)]
pub enum Command {
    /// Start the simulation
    Start,
    
    /// Stop the simulation
    Stop,
    
    /// Pause the simulation
    Pause,
    
    /// Resume from pause
    Resume,
    
    /// Execute a specific number of steps
    Step(usize),
    
    /// Set a breakpoint
    SetBreakpoint(Breakpoint),
    
    /// Remove a breakpoint
    RemoveBreakpoint(BreakpointId),
    
    /// Clear all breakpoints
    ClearBreakpoints,
    
    /// Get current status
    GetStatus,
    
    /// Reset simulation
    Reset,
    
    /// Change configuration
    UpdateConfig(String), // JSON config update
}

/// Responses from the simulation
#[derive(Debug, Clone)]
pub enum Response {
    /// Command acknowledged
    Ok,
    
    /// Command failed
    Error(String),
    
    /// Status response
    Status {
        state: String,
        time: f64,
        step_count: usize,
    },
    
    /// Breakpoint hit
    BreakpointHit {
        id: BreakpointId,
        description: String,
    },
    
    /// Progress update
    Progress {
        percent: f32,
        message: String,
    },
}

/// Control interface for the simulation
pub struct SimulationControl {
    /// Channel for receiving commands
    command_rx: Receiver<Command>,
    
    /// Channel for sending responses
    response_tx: Sender<Response>,
    
    /// Active breakpoints
    breakpoints: HashMap<BreakpointId, Breakpoint>,
    
    /// Next breakpoint ID
    next_breakpoint_id: u32,
}

impl SimulationControl {
    /// Create a new control interface
    pub fn new(command_rx: Receiver<Command>, response_tx: Sender<Response>) -> Self {
        Self {
            command_rx,
            response_tx,
            breakpoints: HashMap::new(),
            next_breakpoint_id: 1,
        }
    }
    
    /// Create a control interface with channels
    pub fn create() -> (Self, Sender<Command>, Receiver<Response>) {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (resp_tx, resp_rx) = crossbeam_channel::unbounded();
        
        let control = Self::new(cmd_rx, resp_tx);
        (control, cmd_tx, resp_rx)
    }
    
    /// Process pending commands
    pub async fn process_commands(&mut self, state_machine: &mut StateMachine) -> SimulationResult<()> {
        // Process all pending commands
        while let Ok(command) = self.command_rx.try_recv() {
            match self.handle_command(command, state_machine).await {
                Ok(response) => {
                    self.response_tx.send(response)
                        .map_err(|e| SimulationError::CommunicationError(e.to_string()))?;
                }
                Err(e) => {
                    self.response_tx.send(Response::Error(e.to_string()))
                        .map_err(|e| SimulationError::CommunicationError(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
    
    /// Handle a single command
    async fn handle_command(
        &mut self,
        command: Command,
        state_machine: &mut StateMachine,
    ) -> SimulationResult<Response> {
        match command {
            Command::Start => {
                state_machine.transition(Event::Start)?;
                state_machine.transition(Event::Run)?;
                Ok(Response::Ok)
            }
            
            Command::Stop => {
                state_machine.transition(Event::Complete)?;
                Ok(Response::Ok)
            }
            
            Command::Pause => {
                state_machine.transition(Event::Pause)?;
                Ok(Response::Ok)
            }
            
            Command::Resume => {
                state_machine.transition(Event::Resume)?;
                Ok(Response::Ok)
            }
            
            Command::Step(_count) => {
                state_machine.transition(Event::Step)?;
                // TODO: Store step count for engine to process
                Ok(Response::Ok)
            }
            
            Command::SetBreakpoint(bp) => {
                let id = BreakpointId::new(self.next_breakpoint_id);
                self.next_breakpoint_id += 1;
                self.breakpoints.insert(id, bp);
                Ok(Response::Ok)
            }
            
            Command::RemoveBreakpoint(id) => {
                self.breakpoints.remove(&id);
                Ok(Response::Ok)
            }
            
            Command::ClearBreakpoints => {
                self.breakpoints.clear();
                Ok(Response::Ok)
            }
            
            Command::GetStatus => {
                Ok(Response::Status {
                    state: state_machine.current_state().to_string(),
                    time: 0.0, // TODO: Get from engine
                    step_count: 0, // TODO: Get from engine
                })
            }
            
            Command::Reset => {
                state_machine.transition(Event::Reset)?;
                Ok(Response::Ok)
            }
            
            Command::UpdateConfig(_config) => {
                // TODO: Parse and apply config update
                Ok(Response::Ok)
            }
        }
    }
    
    /// Check if any breakpoint is hit
    pub fn check_breakpoints(&self, time: f64) -> Option<(BreakpointId, String)> {
        for (id, bp) in &self.breakpoints {
            match bp {
                Breakpoint::TimeBreakpoint(bp_time) => {
                    if time >= *bp_time {
                        return Some((*id, format!("Time breakpoint at {}s", bp_time)));
                    }
                }
                // TODO: Check other breakpoint types
                _ => {}
            }
        }
        None
    }
    
    /// Send a response
    pub fn send_response(&self, response: Response) -> SimulationResult<()> {
        self.response_tx.send(response)
            .map_err(|e| SimulationError::CommunicationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_control_interface() {
        let (mut control, cmd_tx, resp_rx) = SimulationControl::create();
        let mut state_machine = StateMachine::new();
        
        // Send start command
        cmd_tx.send(Command::Start).unwrap();
        
        // Process commands
        control.process_commands(&mut state_machine).await.unwrap();
        
        // Check response
        let response = resp_rx.recv().unwrap();
        assert!(matches!(response, Response::Ok));
        
        // Check state changed
        assert!(state_machine.is_running());
    }
    
    #[test]
    fn test_breakpoints() {
        let (mut control, _, _) = SimulationControl::create();
        
        // Add time breakpoint
        let bp = Breakpoint::TimeBreakpoint(10.0);
        control.breakpoints.insert(BreakpointId::new(1), bp);
        
        // Check before breakpoint
        assert!(control.check_breakpoints(5.0).is_none());
        
        // Check at breakpoint
        let hit = control.check_breakpoints(10.0);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().0, BreakpointId::new(1));
    }
}