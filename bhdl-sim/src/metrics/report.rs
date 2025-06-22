//! Metrics reporting functionality

use std::io::Write;
use crate::metrics::{MetricsCollector, MetricType, MetricValue};
use crate::metrics::stats::SimulationStats;
use crate::error::SimulationResult;

/// Report format options
#[derive(Debug, Clone, Copy)]
pub enum ReportFormat {
    /// Human-readable text format
    Text,
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// HTML format
    Html,
}

/// Metrics report
pub struct MetricsReport {
    /// Simulation statistics
    pub stats: SimulationStats,
    /// Raw metrics
    pub metrics: Vec<(MetricType, MetricValue)>,
    /// Report metadata
    pub metadata: ReportMetadata,
}

/// Report metadata
#[derive(Debug, Clone)]
pub struct ReportMetadata {
    pub title: String,
    pub description: Option<String>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub simulator_version: String,
}

impl Default for ReportMetadata {
    fn default() -> Self {
        Self {
            title: "Simulation Report".to_string(),
            description: None,
            generated_at: chrono::Utc::now(),
            simulator_version: "BHDL Simulator 1.0".to_string(),
        }
    }
}

/// Generate a report from metrics
pub fn generate_report(
    collector: &MetricsCollector,
    stats: &SimulationStats,
    format: ReportFormat,
) -> SimulationResult<String> {
    let report = MetricsReport {
        stats: stats.clone(),
        metrics: collector.get_all_metrics().iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        metadata: ReportMetadata::default(),
    };

    match format {
        ReportFormat::Text => generate_text_report(&report),
        ReportFormat::Json => generate_json_report(&report),
        ReportFormat::Csv => generate_csv_report(&report),
        ReportFormat::Html => generate_html_report(&report),
    }
}

fn generate_text_report(report: &MetricsReport) -> SimulationResult<String> {
    let mut output = String::new();
    
    // Header
    output.push_str(&format!("# {}\n", report.metadata.title));
    output.push_str(&format!("Generated: {}\n", report.metadata.generated_at.format("%Y-%m-%d %H:%M:%S UTC")));
    output.push_str(&format!("Simulator: {}\n\n", report.metadata.simulator_version));
    
    // Statistics summary
    output.push_str(&report.stats.summary());
    
    // Raw metrics
    output.push_str("\n=== Raw Metrics ===\n");
    for (metric_type, value) in &report.metrics {
        output.push_str(&format!("{:?}: {}\n", metric_type, format_metric_value(value)));
    }
    
    Ok(output)
}

fn generate_json_report(report: &MetricsReport) -> SimulationResult<String> {
    use serde_json::json;
    
    let json_report = json!({
        "metadata": {
            "title": report.metadata.title,
            "description": report.metadata.description,
            "generated_at": report.metadata.generated_at.to_rfc3339(),
            "simulator_version": report.metadata.simulator_version,
        },
        "summary": {
            "total_time": report.stats.total_time,
            "total_steps": report.stats.total_steps,
            "total_events": report.stats.total_events,
            "total_evaluations": report.stats.total_evaluations,
            "convergence_failures": report.stats.convergence_failures,
        },
        "performance": {
            "wall_time_seconds": report.stats.performance.wall_time.as_secs_f64(),
            "simulation_speed": report.stats.performance.simulation_speed,
            "peak_memory_mb": report.stats.performance.peak_memory_mb,
            "avg_memory_mb": report.stats.performance.avg_memory_mb,
        },
        "components": report.stats.components.iter().map(|(id, stats)| {
            json!({
                "id": format!("{:?}", id),
                "name": stats.name,
                "type": stats.module_type,
                "evaluation_count": stats.evaluation_count,
                "error_count": stats.error_count,
                "avg_evaluation_time_us": stats.avg_evaluation_time.as_micros(),
            })
        }).collect::<Vec<_>>(),
        "nets": report.stats.nets.iter().map(|(id, stats)| {
            json!({
                "id": format!("{:?}", id),
                "name": stats.name,
                "change_count": stats.change_count,
                "conflict_count": stats.conflict_count,
                "avg_voltage": stats.avg_voltage,
                "peak_current": stats.peak_current,
            })
        }).collect::<Vec<_>>(),
    });
    
    serde_json::to_string_pretty(&json_report)
        .map_err(|e| crate::error::SimulationError::IoError(format!("JSON serialization failed: {}", e)))
}

fn generate_csv_report(report: &MetricsReport) -> SimulationResult<String> {
    let mut output = String::new();
    
    // Summary section
    output.push_str("Section,Metric,Value\n");
    output.push_str(&format!("Summary,Total Time,{}\n", report.stats.total_time));
    output.push_str(&format!("Summary,Total Steps,{}\n", report.stats.total_steps));
    output.push_str(&format!("Summary,Total Events,{}\n", report.stats.total_events));
    output.push_str(&format!("Summary,Total Evaluations,{}\n", report.stats.total_evaluations));
    output.push_str(&format!("Performance,Wall Time,{}\n", report.stats.performance.wall_time.as_secs_f64()));
    output.push_str(&format!("Performance,Simulation Speed,{}\n", report.stats.performance.simulation_speed));
    output.push_str(&format!("Performance,Peak Memory MB,{}\n", report.stats.performance.peak_memory_mb));
    
    // Component data
    output.push_str("\nComponent,Name,Type,Evaluations,Errors\n");
    for (_, stats) in &report.stats.components {
        output.push_str(&format!("{},{},{},{},{}\n",
            stats.name,
            stats.name,
            stats.module_type,
            stats.evaluation_count,
            stats.error_count
        ));
    }
    
    Ok(output)
}

fn generate_html_report(report: &MetricsReport) -> SimulationResult<String> {
    let mut html = String::new();
    
    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str(&format!("<title>{}</title>\n", report.metadata.title));
    html.push_str("<style>\n");
    html.push_str("body { font-family: Arial, sans-serif; margin: 20px; }\n");
    html.push_str("table { border-collapse: collapse; width: 100%; margin: 20px 0; }\n");
    html.push_str("th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }\n");
    html.push_str("th { background-color: #f2f2f2; }\n");
    html.push_str(".metric-section { margin: 20px 0; }\n");
    html.push_str(".metric-value { font-weight: bold; color: #2196F3; }\n");
    html.push_str("</style>\n</head>\n<body>\n");
    
    // Header
    html.push_str(&format!("<h1>{}</h1>\n", report.metadata.title));
    html.push_str(&format!("<p>Generated: {}</p>\n", report.metadata.generated_at.format("%Y-%m-%d %H:%M:%S UTC")));
    
    // Summary metrics
    html.push_str("<div class='metric-section'>\n<h2>Summary</h2>\n");
    html.push_str("<table>\n<tr><th>Metric</th><th>Value</th></tr>\n");
    html.push_str(&format!("<tr><td>Total Simulation Time</td><td class='metric-value'>{:.9}s</td></tr>\n", report.stats.total_time));
    html.push_str(&format!("<tr><td>Total Steps</td><td class='metric-value'>{}</td></tr>\n", report.stats.total_steps));
    html.push_str(&format!("<tr><td>Total Events</td><td class='metric-value'>{}</td></tr>\n", report.stats.total_events));
    html.push_str(&format!("<tr><td>Simulation Speed</td><td class='metric-value'>{:.2}x</td></tr>\n", report.stats.performance.simulation_speed));
    html.push_str("</table>\n</div>\n");
    
    // Component statistics
    html.push_str("<div class='metric-section'>\n<h2>Component Statistics</h2>\n");
    html.push_str("<table>\n<tr><th>Component</th><th>Type</th><th>Evaluations</th><th>Avg Time (µs)</th></tr>\n");
    
    let mut components: Vec<_> = report.stats.components.values().collect();
    components.sort_by_key(|c| std::cmp::Reverse(c.evaluation_count));
    
    for comp in components.iter().take(20) {
        html.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.1}</td></tr>\n",
            comp.name,
            comp.module_type,
            comp.evaluation_count,
            comp.avg_evaluation_time.as_micros() as f64 / 1000.0
        ));
    }
    html.push_str("</table>\n</div>\n");
    
    html.push_str("</body>\n</html>");
    
    Ok(html)
}

fn format_metric_value(value: &MetricValue) -> String {
    match value {
        MetricValue::Integer(i) => i.to_string(),
        MetricValue::Real(r) => format!("{:.6}", r),
        MetricValue::Duration(d) => format!("{:.3}ms", d.as_secs_f64() * 1000.0),
        MetricValue::Text(s) => s.clone(),
        MetricValue::Boolean(b) => b.to_string(),
        MetricValue::List(l) => format!("[{} items]", l.len()),
    }
}

/// Write report to file
pub fn write_report_to_file(
    report_content: &str,
    path: &str,
) -> SimulationResult<()> {
    let mut file = std::fs::File::create(path)
        .map_err(|e| crate::error::SimulationError::IoError(format!("Failed to create report file: {}", e)))?;
    
    file.write_all(report_content.as_bytes())
        .map_err(|e| crate::error::SimulationError::IoError(format!("Failed to write report: {}", e)))?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_report() {
        let collector = MetricsCollector::new();
        let stats = SimulationStats::new();
        
        let report = generate_report(&collector, &stats, ReportFormat::Text).unwrap();
        assert!(report.contains("Simulation Report"));
        assert!(report.contains("Total Time:"));
    }

    #[test]
    fn test_json_report() {
        let mut collector = MetricsCollector::new();
        collector.record(MetricType::TotalSteps, MetricValue::Integer(100));
        
        let mut stats = SimulationStats::new();
        stats.total_time = 1e-6;
        stats.total_steps = 100;
        
        let report = generate_report(&collector, &stats, ReportFormat::Json).unwrap();
        let json: serde_json::Value = serde_json::from_str(&report).unwrap();
        
        assert_eq!(json["summary"]["total_steps"], 100);
        assert_eq!(json["summary"]["total_time"], 1e-6);
    }

    #[test]
    fn test_html_report() {
        let collector = MetricsCollector::new();
        let stats = SimulationStats::new();
        
        let report = generate_report(&collector, &stats, ReportFormat::Html).unwrap();
        assert!(report.contains("<!DOCTYPE html>"));
        assert!(report.contains("<h1>Simulation Report</h1>"));
        assert!(report.contains("<table>"));
    }
}