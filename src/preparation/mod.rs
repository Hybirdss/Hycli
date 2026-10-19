//! Evidence-led preparation: bounded public/first-party reads, never endpoint guessing.
mod grounding;
mod jobs;
#[cfg(test)]
#[path = "tests.rs"]
mod merge_tests;
mod models;
mod orchestrator;
mod prompts;
