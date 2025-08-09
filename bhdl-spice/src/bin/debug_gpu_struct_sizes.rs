//! Debug GPU struct sizes to understand the mismatch

use bhdl_spice::glacier_gpu::gpu_data::{GpuSolverState, Phase0Result};

fn main() {
    println!("GPU Struct Sizes:");
    println!("  GpuSolverState: {} bytes", std::mem::size_of::<GpuSolverState>());
    println!("  Phase0Result:   {} bytes", std::mem::size_of::<Phase0Result>());
    
    // Print field offsets for GpuSolverState
    println!("\nGpuSolverState fields:");
    let state = GpuSolverState {
        iteration: 0,
        converged: 0,
        error: 0.0,
        damping: 0.0,
        integral: 0.0,
        last_error: 0.0,
        filtered_gradient: 0.0,
        _padding: 0.0,
    };
    
    unsafe {
        let base = &state as *const _ as usize;
        println!("  iteration:         offset {} (u32)", &state.iteration as *const _ as usize - base);
        println!("  converged:         offset {} (u32)", &state.converged as *const _ as usize - base);
        println!("  error:             offset {} (f32)", &state.error as *const _ as usize - base);
        println!("  damping:           offset {} (f32)", &state.damping as *const _ as usize - base);
        println!("  integral:          offset {} (f32)", &state.integral as *const _ as usize - base);
        println!("  last_error:        offset {} (f32)", &state.last_error as *const _ as usize - base);
        println!("  filtered_gradient: offset {} (f32)", &state.filtered_gradient as *const _ as usize - base);
        println!("  _padding:          offset {} (f32)", &state._padding as *const _ as usize - base);
    }
    
    println!("\nPhase0Result fields:");
    let result = Phase0Result {
        ramp: 0.0,
        converged: 0,
        iterations: 0,
        error: 0.0,
        max_gradient: 0.0,
        damping: 0.0,
        _padding1: 0.0,
        _padding2: 0.0,
    };
    
    unsafe {
        let base = &result as *const _ as usize;
        println!("  ramp:         offset {} (f32)", &result.ramp as *const _ as usize - base);
        println!("  converged:    offset {} (u32)", &result.converged as *const _ as usize - base);
        println!("  iterations:   offset {} (u32)", &result.iterations as *const _ as usize - base);
        println!("  error:        offset {} (f32)", &result.error as *const _ as usize - base);
        println!("  max_gradient: offset {} (f32)", &result.max_gradient as *const _ as usize - base);
        println!("  damping:      offset {} (f32)", &result.damping as *const _ as usize - base);
        println!("  _padding1:    offset {} (f32)", &result._padding1 as *const _ as usize - base);
        println!("  _padding2:    offset {} (f32)", &result._padding2 as *const _ as usize - base);
    }
}