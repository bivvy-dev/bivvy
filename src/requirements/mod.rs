//! Requirement detection and gap checking.
//!
//! This module provides tools for detecting whether system-level
//! prerequisites (programming languages, databases, tools) are
//! installed and accessible, and for offering to install missing ones.
//!
//! # Modules
//!
//! - [`probe`] - Environment probe for discovering version managers and tools
//! - [`status`] - Requirement status types for gap detection results

pub mod probe;
pub mod status;
