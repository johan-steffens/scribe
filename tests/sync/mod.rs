// Sync integration tests.
//
// These tests verify the sync engine and cloud providers using mocked HTTP
// responses. The `sync` feature must be enabled to compile these tests.

#[cfg(feature = "sync")]
mod engine_tests;

#[cfg(feature = "sync")]
mod provider_tests;
