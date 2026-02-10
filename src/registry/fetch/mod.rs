//! Template fetching from remote sources.
//!
//! This module provides fetchers for HTTP and Git-based template sources.

pub mod git;
pub mod http;

pub use git::{GitFetchResult, GitFetcher};
pub use http::{FetchResponse, HttpFetcher};
