use std::error::Error;
mod standalone {
    include!("standalone_glacier.rs");
}
fn main() -> Result<(), Box<dyn Error>> {
    use standalone::*;
    let circuit = create_sharp_clamp_circuit();
    let mut solver = GlacierSolver::new(circuit);
    solver.max_iterations = 3000;
    match solver.solve_at_ramp(0.95, None) {
        Ok(sol) => {
            println!("Converged in {} iterations, final error {:.3e}", sol.iterations, sol.final_error);
        }
        Err(e) => {
            println!("Solve failed: {}", e);
        }
    }
    Ok(())
}
