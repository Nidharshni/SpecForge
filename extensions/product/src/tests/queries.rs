//! The query functions called directly over a `CommandGraph`, no host.

mod coverage_matrices;
mod dates_and_effort;
mod dependency_graphs;
mod features;
mod lists;
mod milestones_and_journeys;
mod project_wide;
mod rollups;
mod term_analytics;
mod traceability;

/// A small deterministic generator (xorshift64*), so the randomized
/// graphs below are the same every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
