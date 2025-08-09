//! GPU context management for cross-platform compute
//! 
//! Handles GPU device initialization and resource management for
//! wgpu-based compute operations.

use std::sync::Arc;
use wgpu::{Device, Queue, Instance, Adapter, Features, Limits};
use anyhow::{Result, Context};
use log::{info, warn};

/// GPU context for compute operations
#[derive(Clone)]
pub struct GpuContext {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub adapter_info: wgpu::AdapterInfo,
}

impl GpuContext {
    /// Create a new GPU context, preferring high-performance adapters
    pub async fn new() -> Result<Self> {
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request high-performance adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .context("Failed to find suitable GPU adapter")?;

        let adapter_info = adapter.get_info();
        info!("Using GPU: {} ({:?})", adapter_info.name, adapter_info.backend);

        // Check for compute shader support
        let features = adapter.features();
        if !features.contains(Features::SHADER_F64) {
            warn!("GPU does not support 64-bit floats, using f32 with auto-scaling");
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("GLACIER GPU Device"),
                    required_features: Features::empty(),
                    required_limits: Limits::default(),
                },
                None,
            )
            .await
            .context("Failed to create GPU device")?;

        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            adapter_info,
        })
    }

    /// Check if GPU supports required features
    pub fn supports_double_precision(&self) -> bool {
        self.device.features().contains(Features::SHADER_F64)
    }

    /// Get maximum workgroup size
    pub fn max_workgroup_size(&self) -> u32 {
        let limits = self.device.limits();
        limits.max_compute_workgroup_size_x
            .min(limits.max_compute_workgroup_size_y)
            .min(limits.max_compute_workgroup_size_z)
    }

    /// Get maximum buffer size
    pub fn max_buffer_size(&self) -> u64 {
        self.device.limits().max_buffer_size as u64
    }

    /// Check if running on Apple Silicon
    pub fn is_apple_silicon(&self) -> bool {
        matches!(self.adapter_info.backend, wgpu::Backend::Metal)
            && self.adapter_info.name.contains("Apple")
    }
}