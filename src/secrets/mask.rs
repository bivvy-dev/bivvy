//! Output masking for secret values.
//!
//! This module provides functionality for masking sensitive values in output streams.

use std::collections::HashMap;
use std::io::{self, Write};

/// Masks secret values in output streams.
///
/// # Example
///
/// ```
/// use bivvy::secrets::OutputMasker;
///
/// let mut masker = OutputMasker::new();
/// masker.add_secret("super-secret-value");
///
/// let output = masker.mask("The key is super-secret-value here");
/// assert_eq!(output, "The key is [REDACTED] here");
/// assert!(!output.contains("super-secret-value"));
/// ```
pub struct OutputMasker {
    /// Map of secret values to their masked representation.
    secrets: HashMap<String, String>,
    /// The mask string to use.
    mask: String,
}

impl OutputMasker {
    /// Create a new masker with default mask string.
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
            mask: "[REDACTED]".to_string(),
        }
    }

    /// Create a masker with a custom mask string.
    ///
    /// # Example
    ///
    /// ```
    /// use bivvy::secrets::OutputMasker;
    ///
    /// let mut masker = OutputMasker::with_mask("***");
    /// masker.add_secret("password123");
    ///
    /// let output = masker.mask("password: password123");
    /// assert_eq!(output, "password: ***");
    /// ```
    pub fn with_mask(mask: impl Into<String>) -> Self {
        Self {
            secrets: HashMap::new(),
            mask: mask.into(),
        }
    }

    /// Register a secret value to be masked.
    ///
    /// Empty strings are ignored.
    pub fn add_secret(&mut self, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.secrets.insert(value, self.mask.clone());
        }
    }

    /// Register multiple secret values.
    pub fn add_secrets(&mut self, values: impl IntoIterator<Item = impl Into<String>>) {
        for value in values {
            self.add_secret(value);
        }
    }

    /// Mask any secret values in the given string.
    pub fn mask(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (secret, mask) in &self.secrets {
            result = result.replace(secret, mask);
        }
        result
    }

    /// Get the number of registered secrets.
    pub fn secret_count(&self) -> usize {
        self.secrets.len()
    }

    /// Create a writer that masks output.
    ///
    /// # Example
    ///
    /// ```
    /// use bivvy::secrets::OutputMasker;
    /// use std::io::Write;
    ///
    /// let mut masker = OutputMasker::new();
    /// masker.add_secret("secret-value");
    ///
    /// let mut output = Vec::new();
    /// {
    ///     let mut writer = masker.writer(&mut output);
    ///     writeln!(writer, "The value is secret-value").unwrap();
    ///     writer.flush().unwrap();
    /// }
    ///
    /// let result = String::from_utf8(output).unwrap();
    /// assert!(result.contains("[REDACTED]"));
    /// assert!(!result.contains("secret-value"));
    /// ```
    pub fn writer<W: Write>(&self, inner: W) -> MaskingWriter<'_, W> {
        MaskingWriter {
            inner,
            masker: self,
            buffer: String::new(),
        }
    }
}

impl Default for OutputMasker {
    fn default() -> Self {
        Self::new()
    }
}

/// A writer that masks secret values.
///
/// This wraps another writer and replaces any secret values with their
/// masked representation before writing.
pub struct MaskingWriter<'a, W: Write> {
    inner: W,
    masker: &'a OutputMasker,
    buffer: String,
}

impl<W: Write> Write for MaskingWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let input = String::from_utf8_lossy(buf);
        self.buffer.push_str(&input);

        // Process complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..=newline_pos].to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();
            let masked = self.masker.mask(&line);
            self.inner.write_all(masked.as_bytes())?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let masked = self.masker.mask(&self.buffer);
            self.inner.write_all(masked.as_bytes())?;
            self.buffer.clear();
        }
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_single_secret() {
        let mut masker = OutputMasker::new();
        masker.add_secret("super-secret-value");

        let output = masker.mask("The key is super-secret-value here");

        assert_eq!(output, "The key is [REDACTED] here");
        assert!(!output.contains("super-secret-value"));
    }

    #[test]
    fn masks_multiple_secrets() {
        let mut masker = OutputMasker::new();
        masker.add_secret("secret1");
        masker.add_secret("secret2");

        let output = masker.mask("Values: secret1 and secret2");

        assert_eq!(output, "Values: [REDACTED] and [REDACTED]");
    }

    #[test]
    fn ignores_empty_secrets() {
        let mut masker = OutputMasker::new();
        masker.add_secret("");
        masker.add_secret("real-secret");

        let output = masker.mask("The real-secret is here");

        assert_eq!(output, "The [REDACTED] is here");
        assert_eq!(masker.secret_count(), 1);
    }

    #[test]
    fn custom_mask_string() {
        let mut masker = OutputMasker::with_mask("***");
        masker.add_secret("password123");

        let output = masker.mask("password: password123");

        assert_eq!(output, "password: ***");
    }

    #[test]
    fn add_secrets_batch() {
        let mut masker = OutputMasker::new();
        masker.add_secrets(["secret1".to_string(), "secret2".to_string()]);

        assert_eq!(masker.secret_count(), 2);

        let output = masker.mask("secret1 and secret2");
        assert!(!output.contains("secret1"));
        assert!(!output.contains("secret2"));
    }

    #[test]
    fn masking_writer_masks_output() {
        let mut masker = OutputMasker::new();
        masker.add_secret("secret-value");

        let mut output = Vec::new();
        {
            let mut writer = masker.writer(&mut output);
            writeln!(writer, "The value is secret-value").unwrap();
            writer.flush().unwrap();
        }

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("secret-value"));
    }

    #[test]
    fn masking_writer_handles_partial_lines() {
        let mut masker = OutputMasker::new();
        masker.add_secret("secret");

        let mut output = Vec::new();
        {
            let mut writer = masker.writer(&mut output);
            write!(writer, "partial ").unwrap();
            write!(writer, "secret ").unwrap();
            write!(writer, "line").unwrap();
            writer.flush().unwrap();
        }

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("secret"));
    }

    #[test]
    fn masks_multiple_occurrences() {
        let mut masker = OutputMasker::new();
        masker.add_secret("token");

        let output = masker.mask("token=token123, other_token=abc");

        assert_eq!(output, "[REDACTED]=[REDACTED]123, other_[REDACTED]=abc");
    }

    #[test]
    fn default_is_new() {
        let masker = OutputMasker::default();
        assert_eq!(masker.secret_count(), 0);
    }

    #[test]
    fn no_masking_without_secrets() {
        let masker = OutputMasker::new();
        let input = "This has no secrets to mask";

        let output = masker.mask(input);

        assert_eq!(output, input);
    }
}
