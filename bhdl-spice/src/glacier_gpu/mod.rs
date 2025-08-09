//! GPU-accelerated GLACIER solver implementation
//! 
//! Cross-platform GPU implementation using wgpu for:
//! - Apple Silicon (Metal backend)
//! - NVIDIA/AMD GPUs (Vulkan/DirectX backend)
//! - Intel GPUs (Vulkan backend)
//! 
//! Falls back to CPU parallelism with rayon when GPU is unavailable.

pub mod gpu_context;
pub mod phase0_gpu;
pub mod multiregion_gpu;
pub mod solver;
pub mod gpu_data;
pub mod matrix_ops;
pub mod full_solver;
pub mod auto_scaling;
pub mod region_detection;
pub mod hybrid_solver;

pub use solver::GlacierGpuSolver;
pub use gpu_context::GpuContext;
pub use full_solver::GlacierFullGpuSolver;
pub use region_detection::{detect_gradient_regions, GpuRegion};