//! Layer stack presets and auto-inference.

use crate::types::*;

/// Build a layer stack from a standard preset.
pub fn stackup_preset(preset: StackupPreset) -> LayerStack {
    match preset {
        StackupPreset::TwoLayer => LayerStack {
            // Simple: Signal / Signal
            // Use case: Hobby, low-complexity, cost-sensitive
            layers: vec![
                Layer {
                    id: 0,
                    name: "F.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
                Layer {
                    id: 1,
                    name: "B.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
            ],
            dielectrics: vec![Dielectric {
                thickness_mm: 1.53,
                material: "FR4".into(),
                er: 4.3,
                loss_tangent: 0.02,
            }],
            total_thickness_mm: 1.6,
            via: ViaSpec {
                drill_mm: 0.3,
                pad_mm: 0.6,
                annular_ring_mm: 0.15,
            },
        },

        StackupPreset::FourLayer => LayerStack {
            // Standard professional: Signal / Ground / Power / Signal
            // Use case: Most production boards, moderate complexity
            layers: vec![
                Layer {
                    id: 0,
                    name: "F.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
                Layer {
                    id: 1,
                    name: "In1.Cu".into(),
                    kind: LayerKind::Ground,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.0,
                },
                Layer {
                    id: 2,
                    name: "In2.Cu".into(),
                    kind: LayerKind::Power,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.0,
                },
                Layer {
                    id: 3,
                    name: "B.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
            ],
            dielectrics: vec![
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 1.20,
                    material: "Core".into(),
                    er: 4.3,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec {
                drill_mm: 0.3,
                pad_mm: 0.6,
                annular_ring_mm: 0.15,
            },
        },

        StackupPreset::SixLayer => LayerStack {
            // Complex mixed-signal: Signal / Ground / Signal / Signal / Power / Signal
            layers: vec![
                Layer {
                    id: 0,
                    name: "F.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
                Layer {
                    id: 1,
                    name: "In1.Cu".into(),
                    kind: LayerKind::Ground,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.0,
                },
                Layer {
                    id: 2,
                    name: "In2.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.8,
                },
                Layer {
                    id: 3,
                    name: "In3.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.8,
                },
                Layer {
                    id: 4,
                    name: "In4.Cu".into(),
                    kind: LayerKind::Power,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.0,
                },
                Layer {
                    id: 5,
                    name: "B.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
            ],
            dielectrics: vec![
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.20,
                    material: "Core".into(),
                    er: 4.3,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.56,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.20,
                    material: "Core".into(),
                    er: 4.3,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec {
                // 0.25 drill sat below KiCad's default 0.3mm min-hole
                // constraint — every via on a 4-layer board flagged
                // drill_out_of_range. Standard 0.6/0.3 via.
                drill_mm: 0.3,
                pad_mm: 0.6,
                annular_ring_mm: 0.15,
            },
        },

        StackupPreset::EightLayer => LayerStack {
            // High-speed/dense: Sig / GND / Sig / Sig / Sig / Sig / PWR / Sig
            layers: vec![
                Layer {
                    id: 0,
                    name: "F.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
                Layer {
                    id: 1,
                    name: "In1.Cu".into(),
                    kind: LayerKind::Ground,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.0,
                },
                Layer {
                    id: 2,
                    name: "In2.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.8,
                },
                Layer {
                    id: 3,
                    name: "In3.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.8,
                },
                Layer {
                    id: 4,
                    name: "In4.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.8,
                },
                Layer {
                    id: 5,
                    name: "In5.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.8,
                },
                Layer {
                    id: 6,
                    name: "In6.Cu".into(),
                    kind: LayerKind::Power,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 0.0,
                },
                Layer {
                    id: 7,
                    name: "B.Cu".into(),
                    kind: LayerKind::Signal,
                    thickness_mm: 0.035,
                    copper_weight_oz: 1.0,
                    dielectric_constant: 4.3,
                    capacity_factor: 1.0,
                },
            ],
            dielectrics: vec![
                Dielectric {
                    thickness_mm: 0.075,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Core".into(),
                    er: 4.3,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.36,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Core".into(),
                    er: 4.3,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.36,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.10,
                    material: "Core".into(),
                    er: 4.3,
                    loss_tangent: 0.02,
                },
                Dielectric {
                    thickness_mm: 0.075,
                    material: "Prepreg".into(),
                    er: 4.2,
                    loss_tangent: 0.02,
                },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec {
                drill_mm: 0.2,
                pad_mm: 0.45,
                annular_ring_mm: 0.125,
            },
        },
    }
}

/// Infer layer count from circuit complexity.
pub fn infer_layer_count(
    num_components: usize,
    num_nets: usize,
    num_power_domains: usize,
    has_high_speed: bool,
) -> StackupPreset {
    if num_components <= 15 && num_nets <= 20 && !has_high_speed {
        StackupPreset::TwoLayer
    } else if num_components <= 100 && num_power_domains <= 4 && !has_high_speed {
        StackupPreset::FourLayer
    } else if num_components <= 300 || has_high_speed {
        StackupPreset::SixLayer
    } else {
        StackupPreset::EightLayer
    }
}

/// Resolve a StackupSource into a concrete LayerStack.
pub fn resolve_stackup(
    source: &StackupSource,
    num_components: usize,
    num_nets: usize,
    num_power_domains: usize,
    has_high_speed: bool,
) -> LayerStack {
    match source {
        StackupSource::Preset(preset) => stackup_preset(*preset),
        StackupSource::Auto => {
            let preset =
                infer_layer_count(num_components, num_nets, num_power_domains, has_high_speed);
            stackup_preset(preset)
        }
        StackupSource::Custom(stack) => stack.clone(),
    }
}

/// Trace width from current (IPC-2221 formula).
///
/// Returns trace width in mm for given current, copper weight, and temperature rise.
/// Inverse of [`trace_width_for_current`] at 1oz/10°C: the current a
/// given width carries under the same IPC-2221 model. Used by the
/// power-tree flow analysis to recover the net's rail current from the
/// classifier's computed width.
pub fn current_for_trace_width(width_mm: f64) -> f64 {
    if width_mm <= 0.15 {
        return 0.0;
    }
    let width_mils = width_mm / 0.0254;
    let thickness_mils = 1.378; // 1oz
    let area_mils2 = width_mils * thickness_mils;
    0.024 * 10f64.powf(0.44) * area_mils2.powf(0.725)
}

pub fn trace_width_for_current(current_a: f64, copper_oz: f64, temp_rise_c: f64) -> f64 {
    if current_a <= 0.0 {
        return 0.15; // default minimum
    }
    // IPC-2221 internal layer formula
    // A = I / (k * ΔT^b)^(1/c)  where k=0.024, b=0.44, c=0.725
    let area_mils2 = (current_a / (0.024 * temp_rise_c.powf(0.44))).powf(1.0 / 0.725);
    let thickness_mils = copper_oz * 1.378; // 1oz = 1.378 mils
    let width_mils = area_mils2 / thickness_mils;
    let width_mm = width_mils * 0.0254;
    width_mm.max(0.15) // never below minimum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_layer_stackup() {
        let stack = stackup_preset(StackupPreset::TwoLayer);
        assert_eq!(stack.layers.len(), 2);
        assert_eq!(stack.signal_layer_indices(), vec![0, 1]);
        assert!(stack.inner_signal_layer_indices().is_empty());
    }

    #[test]
    fn test_four_layer_stackup() {
        let stack = stackup_preset(StackupPreset::FourLayer);
        assert_eq!(stack.layers.len(), 4);
        assert_eq!(stack.signal_layer_indices(), vec![0, 3]);
        assert_eq!(stack.layers_adjacent_to(LayerKind::Ground), vec![0]);
    }

    #[test]
    fn test_six_layer_stackup() {
        let stack = stackup_preset(StackupPreset::SixLayer);
        assert_eq!(stack.layers.len(), 6);
        assert_eq!(stack.signal_layer_indices(), vec![0, 2, 3, 5]);
        assert_eq!(stack.inner_signal_layer_indices(), vec![2, 3]);
    }

    #[test]
    fn test_layer_inference() {
        assert_eq!(infer_layer_count(10, 15, 1, false), StackupPreset::TwoLayer);
        assert_eq!(infer_layer_count(50, 80, 3, false), StackupPreset::FourLayer);
        assert_eq!(
            infer_layer_count(200, 300, 5, false),
            StackupPreset::SixLayer
        );
        assert_eq!(
            infer_layer_count(50, 80, 2, true),
            StackupPreset::SixLayer
        );
    }

    #[test]
    fn test_trace_width_ipc2221() {
        // 1A on 1oz copper with 10°C rise should be ~0.25mm
        let w = trace_width_for_current(1.0, 1.0, 10.0);
        assert!(w > 0.15 && w < 1.0, "trace width = {w}");

        // 0A should return minimum
        assert_eq!(trace_width_for_current(0.0, 1.0, 10.0), 0.15);
    }
}
