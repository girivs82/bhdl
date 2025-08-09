//! Test GPU availability

use anyhow::Result;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::GpuContext;

#[cfg(feature = "gpu")]
async fn check_gpu() -> Result<()> {
    println!("Checking GPU availability...");
    
    match GpuContext::new().await {
        Ok(context) => {
            println!("✓ GPU is available!");
            println!("  Adapter: {}", context.adapter_info.name);
            println!("  Backend: {:?}", context.adapter_info.backend);
            println!("  Driver: {}", context.adapter_info.driver);
            println!("  Driver info: {}", context.adapter_info.driver_info);
            Ok(())
        }
        Err(e) => {
            println!("✗ GPU not available: {}", e);
            println!("\nThis is expected if:");
            println!("  - Running in a headless environment");
            println!("  - No GPU drivers installed");
            println!("  - WebGPU/wgpu not supported on this system");
            Err(e)
        }
    }
}

#[cfg(not(feature = "gpu"))]
async fn check_gpu() -> Result<()> {
    println!("GPU feature not enabled. Build with --features gpu");
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(check_gpu())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Build with --features gpu");
        Ok(())
    }
}