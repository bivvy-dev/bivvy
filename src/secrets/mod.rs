//! Secret detection and masking.
//!
//! This module provides functionality for detecting and masking sensitive data:
//!
//! - [`SecretPattern`] - Defines a pattern for identifying secrets
//! - [`SecretMatcher`] - Matches environment variable names against secret patterns
//! - [`OutputMasker`] - Masks secret values in output streams
//! - [`BUILTIN_PATTERNS`] - Built-in patterns for common secrets
//!
//! # Example
//!
//! ```
//! use bivvy::secrets::{SecretMatcher, OutputMasker};
//!
//! // Check if an environment variable is a secret
//! let matcher = SecretMatcher::with_builtins();
//! assert!(matcher.is_secret("API_KEY"));
//! assert!(matcher.is_secret("DATABASE_URL"));
//! assert!(!matcher.is_secret("PATH"));
//!
//! // Mask secret values in output
//! let mut masker = OutputMasker::new();
//! masker.add_secret("super-secret-value");
//! let output = masker.mask("The key is super-secret-value here");
//! assert!(!output.contains("super-secret-value"));
//! ```

pub mod mask;
pub mod pattern;

pub use mask::{MaskingWriter, OutputMasker};
pub use pattern::{SecretMatcher, SecretPattern, BUILTIN_PATTERNS};
