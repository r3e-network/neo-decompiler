//! Analysis report generation

use crate::core::ir::*;
use serde::{Deserialize, Serialize};

/// Report generator for analysis results
pub struct ReportGenerator;

impl ReportGenerator {
    /// Generate analysis report for IR function
    pub fn generate_report(&self, function: &IRFunction) -> AnalysisReport {
        AnalysisReport {
            function_name: function.name.clone(),
            complexity: function.metadata.complexity.clone(),
            security: function.metadata.complexity.clone().into(),
            summary: self.generate_summary(function),
        }
    }

    /// Generate function summary
    fn generate_summary(&self, function: &IRFunction) -> String {
        format!(
            "Function '{}' has {} blocks with {} total operations. Cyclomatic complexity: {}.",
            function.name,
            function.blocks.len(),
            function.metadata.complexity.operations,
            function.metadata.complexity.cyclomatic
        )
    }
}

/// Analysis report structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    /// Function name
    pub function_name: String,
    /// Complexity metrics
    pub complexity: ComplexityMetrics,
    /// Security analysis
    pub security: ComplexityMetrics, // Security metrics computed during analysis
    /// Summary text
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_generator() {
        let generator = ReportGenerator;
        let function = IRFunction::new("test".to_string());
        
        let report = generator.generate_report(&function);
        assert_eq!(report.function_name, "test");
        assert!(report.summary.contains("test"));
    }
}