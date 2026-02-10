//! Table rendering for formatted output.

/// A simple table for formatted output.
#[derive(Debug)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    column_widths: Vec<usize>,
}

impl Table {
    /// Create a new table with the given headers.
    pub fn new(headers: Vec<&str>) -> Self {
        let headers: Vec<String> = headers.iter().map(|s| s.to_string()).collect();
        let column_widths = headers.iter().map(|h| h.len()).collect();

        Self {
            headers,
            rows: Vec::new(),
            column_widths,
        }
    }

    /// Add a row to the table.
    pub fn add_row(&mut self, row: Vec<&str>) {
        let row: Vec<String> = row.iter().map(|s| s.to_string()).collect();

        // Update column widths
        for (i, cell) in row.iter().enumerate() {
            if i < self.column_widths.len() {
                self.column_widths[i] = self.column_widths[i].max(cell.len());
            }
        }

        self.rows.push(row);
    }

    /// Get the number of rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Check if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Render the table as a string.
    pub fn render(&self) -> String {
        let mut output = String::new();

        // Top border
        output.push_str(&self.render_border('┌', '┬', '┐'));
        output.push('\n');

        // Header row
        output.push_str(&self.render_row(&self.headers));
        output.push('\n');

        // Header separator
        output.push_str(&self.render_border('├', '┼', '┤'));
        output.push('\n');

        // Data rows
        for row in &self.rows {
            output.push_str(&self.render_row(row));
            output.push('\n');
        }

        // Bottom border
        output.push_str(&self.render_border('└', '┴', '┘'));

        output
    }

    fn render_border(&self, left: char, mid: char, right: char) -> String {
        let mut s = String::new();
        s.push(left);

        for (i, width) in self.column_widths.iter().enumerate() {
            s.push_str(&"─".repeat(width + 2));
            if i < self.column_widths.len() - 1 {
                s.push(mid);
            }
        }

        s.push(right);
        s
    }

    fn render_row(&self, row: &[String]) -> String {
        let mut s = String::from("│");

        for (i, width) in self.column_widths.iter().enumerate() {
            let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
            s.push_str(&format!(" {:width$} │", cell, width = width));
        }

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_empty() {
        let table = Table::new(vec!["A", "B"]);
        assert!(table.is_empty());
        assert_eq!(table.row_count(), 0);

        let output = table.render();
        assert!(output.contains("A"));
        assert!(output.contains("B"));
    }

    #[test]
    fn table_with_rows() {
        let mut table = Table::new(vec!["Name", "Status"]);
        table.add_row(vec!["step1", "Success"]);
        table.add_row(vec!["step2", "Failed"]);

        assert_eq!(table.row_count(), 2);

        let output = table.render();
        assert!(output.contains("step1"));
        assert!(output.contains("Success"));
        assert!(output.contains("step2"));
        assert!(output.contains("Failed"));
    }

    #[test]
    fn table_adjusts_column_width() {
        let mut table = Table::new(vec!["A"]);
        table.add_row(vec!["longer_value"]);

        let output = table.render();
        assert!(output.contains("longer_value"));
    }

    #[test]
    fn table_uses_box_drawing() {
        let table = Table::new(vec!["Test"]);
        let output = table.render();

        assert!(output.contains("┌"));
        assert!(output.contains("┐"));
        assert!(output.contains("└"));
        assert!(output.contains("┘"));
        assert!(output.contains("│"));
        assert!(output.contains("─"));
    }

    #[test]
    fn table_handles_missing_cells() {
        let mut table = Table::new(vec!["A", "B", "C"]);
        table.add_row(vec!["only", "two"]);

        let output = table.render();
        assert!(output.contains("only"));
        assert!(output.contains("two"));
    }

    #[test]
    fn table_multiple_columns_separators() {
        let mut table = Table::new(vec!["Col1", "Col2", "Col3"]);
        table.add_row(vec!["a", "b", "c"]);

        let output = table.render();
        // Should have column separators
        assert!(output.contains("┬"));
        assert!(output.contains("┼"));
        assert!(output.contains("┴"));
    }

    #[test]
    fn table_render_consistency() {
        let mut table = Table::new(vec!["Step", "Duration", "Status"]);
        table.add_row(vec!["setup", "1.2s", "✓"]);
        table.add_row(vec!["build", "45.3s", "✓"]);
        table.add_row(vec!["test", "5.0s", "✗"]);

        let output = table.render();
        let lines: Vec<_> = output.lines().collect();

        // Should have: top border, header, separator, 3 data rows, bottom border
        assert_eq!(lines.len(), 7);
    }
}
