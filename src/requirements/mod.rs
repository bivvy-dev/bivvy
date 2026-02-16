//! Requirement detection and gap checking.
//!
//! This module provides tools for detecting whether system-level
//! prerequisites (programming languages, databases, tools) are
//! installed and accessible, and for offering to install missing ones.
//!
//! # Modules
//!
//! - [`checker`] - Gap checker for requirement evaluation
//! - [`probe`] - Environment probe for discovering version managers and tools
//! - [`registry`] - Requirement definitions and registry
//! - [`status`] - Requirement status types for gap detection results

pub mod checker;
pub mod probe;
pub mod registry;
pub mod status;
