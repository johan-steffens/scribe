// Common test helpers for integration and unit tests.
//
// These modules provide reusable utilities for creating temporary databases
// and mock configurations without requiring disk I/O or external services.
//
// # Example
//
// ```ignore
// use scribe::testing::db::TestDb;
// use scribe::testing::config::TestConfig;
//
// let test_db = TestDb::new();
// let config = TestConfig::new();
// ```
//
// Re-exports child modules for convenient access.
pub mod config;
pub mod db;
