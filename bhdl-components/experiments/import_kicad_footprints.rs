//! KiCad Footprint Import Script
//!
//! Imports KiCad footprint files (.kicad_mod) into the bhdl-components database

use std::path::{Path, PathBuf};
use anyhow::{Result, Context, bail};
use clap::{Arg, Command};
use log::{info, error, debug};

use bhdl_components::ComponentDatabase;
use bhdl_components::kicad::parser::KiCadFootprintParser;
use bhdl_components::types::{ComponentFootprint, FootprintPad, PadShape, PadType};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let matches = Command::new("import_kicad_footprints")
        .about("Import KiCad footprint files into bhdl-components database")
        .arg(Arg::new("footprint")
             .help("Path to KiCad footprint file (.kicad_mod) or directory containing footprints")
             .required(true)
             .index(1))
        .arg(Arg::new("component-name")
             .help("Component name to associate footprint with")
             .short('c')
             .long("component")
             .required(true))
        .arg(Arg::new("database")
             .help("Path to components database")
             .short('d')
             .long("database")
             .default_value("components.db"))
        .arg(Arg::new("dry-run")
             .help("Parse and process but don't write to database")
             .long("dry-run")
             .action(clap::ArgAction::SetTrue))
        .get_matches();

    let footprint_path = matches.get_one::<String>("footprint").unwrap();
    let component_name = matches.get_one::<String>("component-name").unwrap();
    let database_path = matches.get_one::<String>("database").unwrap();
    let dry_run = matches.get_flag("dry-run");

    info!("🔧 KiCad Footprint Import Tool");
    info!("Footprint: {}", footprint_path);
    info!("Component: {}", component_name);
    info!("Database: {}", database_path);
    if dry_run {
        info!("DRY RUN MODE - No database changes will be made");
    }

    // Initialize database
    let db_path = Path::new(database_path);
    let database = ComponentDatabase::new(db_path).await
        .context("Failed to initialize component database")?;

    // Initialize parser
    let parser = KiCadFootprintParser::new();

    // Check if path is a file or directory
    let path = Path::new(footprint_path);
    let footprint_files = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        // Find all .kicad_mod files in directory
        std::fs::read_dir(path)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "kicad_mod"))
            .collect()
    } else {
        bail!("Path does not exist or is not a file/directory: {}", footprint_path);
    };

    info!("📖 Found {} footprint file(s) to process", footprint_files.len());

    let mut imported_count = 0;
    let mut error_count = 0;

    // Process each footprint file
    for footprint_file in &footprint_files {
        let file_name = footprint_file.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        debug!("Processing footprint file: {}", file_name);

        match process_footprint(
            &footprint_file,
            component_name,
            &parser,
            &database,
            dry_run
        ).await {
            Ok(_) => {
                imported_count += 1;
                info!("✅ Imported footprint: {}", file_name);
            }
            Err(e) => {
                error_count += 1;
                error!("❌ Failed to import footprint {}: {}", file_name, e);
            }
        }
    }

    info!("🎉 Import complete!");
    info!("   Successfully imported: {}", imported_count);
    info!("   Errors: {}", error_count);
    info!("   Total footprints processed: {}", footprint_files.len());

    if dry_run {
        info!("   (DRY RUN - No actual database changes made)");
    }

    Ok(())
}

/// Process a single KiCad footprint file
async fn process_footprint(
    footprint_file: &Path,
    component_name: &str,
    parser: &KiCadFootprintParser,
    database: &ComponentDatabase,
    dry_run: bool,
) -> Result<()> {

    // Read footprint file
    let content = std::fs::read_to_string(footprint_file)
        .with_context(|| format!("Failed to read footprint file: {:?}", footprint_file))?;

    // Parse footprint
    debug!("Parsing footprint file: {:?}", footprint_file);
    let kicad_footprint = parser.parse_footprint(&content)
        .context("Failed to parse KiCad footprint")?;

    // Convert to ComponentFootprint
    let component_footprint = convert_kicad_footprint(&kicad_footprint)
        .context("Failed to convert KiCad footprint to ComponentFootprint")?;

    if dry_run {
        info!("DRY RUN: Would import footprint '{}' with {} pads for component '{}'",
              component_footprint.footprint_name,
              component_footprint.pad_count,
              component_name);
        return Ok(());
    }

    // Look up component in database
    let component_id = database.get_component_id_by_name(component_name).await
        .with_context(|| format!("Component '{}' not found in database. Import the component symbol first.", component_name))?;

    // Insert footprint into database
    debug!("Inserting footprint for component ID {}", component_id);
    database.insert_component_footprint(component_id, &component_footprint).await
        .context("Failed to insert footprint into database")?;

    debug!("Successfully inserted footprint for component '{}'", component_name);
    Ok(())
}

/// Convert KiCad footprint to ComponentFootprint
fn convert_kicad_footprint(
    kicad_fp: &bhdl_components::kicad::parser::KiCadFootprint
) -> Result<ComponentFootprint> {

    // Convert pads
    let pads: Vec<FootprintPad> = kicad_fp.pads.iter().map(|kicad_pad| {
        FootprintPad {
            pad_number: kicad_pad.number.clone(),
            x_position: kicad_pad.x,
            y_position: kicad_pad.y,
            width: kicad_pad.size_x,
            height: kicad_pad.size_y,
            shape: convert_pad_shape(&kicad_pad.shape),
            drill_diameter: kicad_pad.drill,
            drill_slot: None,
            pad_type: convert_pad_type(&kicad_pad.pad_type),
        }
    }).collect();

    let pad_count = pads.len() as u32;

    // Calculate bounding box dimensions
    let (body_width, body_height) = calculate_body_dimensions(&pads, &kicad_fp.graphics);

    // Calculate pitch (spacing between pads)
    let pitch = calculate_pitch(&pads);

    // Generate simple SVG representation
    let svg_data = generate_footprint_svg(kicad_fp, body_width, body_height);

    Ok(ComponentFootprint {
        footprint_name: kicad_fp.name.clone(),
        svg_data,
        pad_count,
        body_width,
        body_height,
        pitch,
        pads,
    })
}

/// Convert KiCad pad shape to PadShape enum
fn convert_pad_shape(kicad_shape: &str) -> PadShape {
    match kicad_shape {
        "circle" => PadShape::Circle,
        "rect" => PadShape::Rectangle,
        "oval" => PadShape::Oval,
        "roundrect" => PadShape::RoundedRectangle,
        _ => PadShape::Rectangle, // Default to rectangle
    }
}

/// Convert KiCad pad type to PadType enum
fn convert_pad_type(kicad_type: &str) -> PadType {
    match kicad_type {
        "smd" => PadType::SMD,
        "thru_hole" => PadType::ThroughHole,
        "np_thru_hole" => PadType::NPTH,
        _ => PadType::SMD, // Default to SMD
    }
}

/// Calculate body dimensions from pads and graphics
fn calculate_body_dimensions(
    pads: &[FootprintPad],
    graphics: &[bhdl_components::kicad::parser::KiCadFootprintGraphic],
) -> (f64, f64) {
    use bhdl_components::kicad::parser::KiCadFootprintGraphic;

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    // Consider pad positions
    for pad in pads {
        let pad_min_x = pad.x_position - pad.width / 2.0;
        let pad_max_x = pad.x_position + pad.width / 2.0;
        let pad_min_y = pad.y_position - pad.height / 2.0;
        let pad_max_y = pad.y_position + pad.height / 2.0;

        min_x = min_x.min(pad_min_x);
        max_x = max_x.max(pad_max_x);
        min_y = min_y.min(pad_min_y);
        max_y = max_y.max(pad_max_y);
    }

    // Consider graphic elements (for body outline)
    for graphic in graphics {
        match graphic {
            KiCadFootprintGraphic::Line { start_x, start_y, end_x, end_y, .. } => {
                min_x = min_x.min(*start_x).min(*end_x);
                max_x = max_x.max(*start_x).max(*end_x);
                min_y = min_y.min(*start_y).min(*end_y);
                max_y = max_y.max(*start_y).max(*end_y);
            }
            KiCadFootprintGraphic::Circle { center_x, center_y, end_x, end_y, .. } => {
                let radius = ((end_x - center_x).powi(2) + (end_y - center_y).powi(2)).sqrt();
                min_x = min_x.min(center_x - radius);
                max_x = max_x.max(center_x + radius);
                min_y = min_y.min(center_y - radius);
                max_y = max_y.max(center_y + radius);
            }
            KiCadFootprintGraphic::Polygon { points, .. } => {
                for (x, y) in points {
                    min_x = min_x.min(*x);
                    max_x = max_x.max(*x);
                    min_y = min_y.min(*y);
                    max_y = max_y.max(*y);
                }
            }
            _ => {}
        }
    }

    // If no valid bounds found, use a default size
    if min_x.is_infinite() || max_x.is_infinite() {
        return (5.0, 5.0); // Default 5mm x 5mm
    }

    let width = (max_x - min_x).abs();
    let height = (max_y - min_y).abs();

    (width, height)
}

/// Calculate pitch (spacing between pads)
fn calculate_pitch(pads: &[FootprintPad]) -> Option<f64> {
    if pads.len() < 2 {
        return None;
    }

    // Calculate distance between first two pads as pitch estimate
    let pad1 = &pads[0];
    let pad2 = &pads[1];

    let dx = pad2.x_position - pad1.x_position;
    let dy = pad2.y_position - pad1.y_position;
    let distance = (dx * dx + dy * dy).sqrt();

    if distance > 0.001 {
        Some(distance)
    } else {
        None
    }
}

/// Generate simple SVG representation of footprint
fn generate_footprint_svg(
    kicad_fp: &bhdl_components::kicad::parser::KiCadFootprint,
    width: f64,
    height: f64,
) -> String {
    use bhdl_components::kicad::parser::KiCadFootprintGraphic;

    let margin = 2.0;
    let viewbox_width = width + 2.0 * margin;
    let viewbox_height = height + 2.0 * margin;
    let offset_x = width / 2.0 + margin;
    let offset_y = height / 2.0 + margin;

    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}">"#,
        -offset_x, -offset_y, viewbox_width, viewbox_height
    );

    // Draw pads
    for pad in &kicad_fp.pads {
        let x = pad.x - pad.size_x / 2.0;
        let y = pad.y - pad.size_y / 2.0;

        match pad.shape.as_str() {
            "circle" => {
                let radius = pad.size_x / 2.0;
                svg.push_str(&format!(
                    r#"<circle cx="{}" cy="{}" r="{}" fill="gold" stroke="black" stroke-width="0.1"/>"#,
                    pad.x, pad.y, radius
                ));
            }
            _ => {
                svg.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="gold" stroke="black" stroke-width="0.1"/>"#,
                    x, y, pad.size_x, pad.size_y
                ));
            }
        }

        // Add pad number label
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" font-size="0.8" text-anchor="middle" dominant-baseline="middle" fill="black">{}</text>"#,
            pad.x, pad.y, pad.number
        ));
    }

    // Draw graphics on silkscreen layer
    for graphic in &kicad_fp.graphics {
        if let Some(layer) = get_graphic_layer(graphic) {
            if layer.contains("SilkS") || layer.contains("Fab") {
                match graphic {
                    KiCadFootprintGraphic::Line { start_x, start_y, end_x, end_y, stroke_width, .. } => {
                        svg.push_str(&format!(
                            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="white" stroke-width="{}"/>"#,
                            start_x, start_y, end_x, end_y, stroke_width
                        ));
                    }
                    KiCadFootprintGraphic::Circle { center_x, center_y, end_x, end_y, stroke_width, .. } => {
                        let radius = ((end_x - center_x).powi(2) + (end_y - center_y).powi(2)).sqrt();
                        svg.push_str(&format!(
                            r#"<circle cx="{}" cy="{}" r="{}" fill="none" stroke="white" stroke-width="{}"/>"#,
                            center_x, center_y, radius, stroke_width
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    svg.push_str("</svg>");
    svg
}

/// Get layer from graphic element
fn get_graphic_layer(graphic: &bhdl_components::kicad::parser::KiCadFootprintGraphic) -> Option<&str> {
    use bhdl_components::kicad::parser::KiCadFootprintGraphic;

    match graphic {
        KiCadFootprintGraphic::Line { layer, .. } => Some(layer),
        KiCadFootprintGraphic::Circle { layer, .. } => Some(layer),
        KiCadFootprintGraphic::Arc { layer, .. } => Some(layer),
        KiCadFootprintGraphic::Text { layer, .. } => Some(layer),
        KiCadFootprintGraphic::Polygon { layer, .. } => Some(layer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_shape_conversion() {
        assert!(matches!(convert_pad_shape("circle"), PadShape::Circle));
        assert!(matches!(convert_pad_shape("rect"), PadShape::Rectangle));
        assert!(matches!(convert_pad_shape("oval"), PadShape::Oval));
        assert!(matches!(convert_pad_shape("roundrect"), PadShape::RoundedRectangle));
    }

    #[test]
    fn test_pad_type_conversion() {
        assert!(matches!(convert_pad_type("smd"), PadType::SMD));
        assert!(matches!(convert_pad_type("thru_hole"), PadType::ThroughHole));
        assert!(matches!(convert_pad_type("np_thru_hole"), PadType::NPTH));
    }
}
