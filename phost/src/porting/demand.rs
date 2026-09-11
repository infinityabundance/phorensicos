// porting/demand.rs — demand-driven porting, as data (§20)
//
// Phase 8. A runtime miss must not become a blocking reconstruction. It emits
// **data**: a `PortDemand` record describing the surface that was requested, the
// generation the caller was bound to, and the dependencies that were available.
// The host-side foundry may later attempt a reconstruction; a successful campaign
// publishes a future generation. Nothing on the runtime path ever waits for a
// compiler, an oracle, an agent or a network.
//
// The sink is deliberately non-blocking and bounded: recording a demand appends
// to an in-memory buffer and never fails the call. Beyond the cap, records are
// dropped and counted deterministically, so a demand storm cannot grow without
// bound or change a call's behavior.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

/// The default bound on buffered demands.
pub const DEFAULT_DEMAND_CAP: usize = 4096;

/// One observed runtime miss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortDemand {
    /// The port id that was requested and not served natively.
    pub requested_surface: String,
    /// The callsite family, when known (`dispatch` otherwise).
    pub callsite_family: String,
    /// How many times this surface has been demanded (this record may aggregate).
    pub demand_count: u64,
    /// The store generation the caller was bound to.
    pub store_generation: String,
    /// The dependencies available to a reconstruction (sealed port targets).
    pub available_dependencies: Vec<String>,
}

impl PortDemand {
    /// The deterministic canonical string of one demand (used to aggregate and to
    /// persist). No wall clock participates.
    pub fn canonical(&self) -> String {
        let mut deps = self.available_dependencies.clone();
        deps.sort();
        alloc::format!(
            "surface={};callsite={};count={};generation={};deps={}",
            self.requested_surface,
            self.callsite_family,
            self.demand_count,
            self.store_generation,
            deps.join(",")
        )
    }

    /// Aggregate a fresh demand into `self` (same surface + callsite + generation).
    pub fn merge(&mut self, other: &PortDemand) {
        self.demand_count = self.demand_count.saturating_add(other.demand_count);
        for d in &other.available_dependencies {
            if !self.available_dependencies.contains(d) {
                self.available_dependencies.push(d.clone());
            }
        }
    }
}

/// A bounded, non-blocking collector of demands.
pub struct DemandSink {
    cap: usize,
    buffer: RefCell<VecDeque<PortDemand>>,
    dropped: RefCell<u64>,
}

impl DemandSink {
    pub fn new(cap: usize) -> Self {
        DemandSink {
            cap: cap.max(1),
            buffer: RefCell::new(VecDeque::new()),
            dropped: RefCell::new(0),
        }
    }

    /// Record a demand. Never blocks and never fails; beyond the cap the record
    /// is dropped and counted.
    pub fn record(&self, mut demand: PortDemand) {
        let mut buf = self.buffer.borrow_mut();
        // Aggregate into an existing record for the same surface/callsite/generation.
        if let Some(existing) = buf.iter_mut().find(|d| {
            d.requested_surface == demand.requested_surface
                && d.callsite_family == demand.callsite_family
                && d.store_generation == demand.store_generation
        }) {
            existing.merge(&demand);
            return;
        }
        if buf.len() >= self.cap {
            *self.dropped.borrow_mut() += 1;
            return;
        }
        // Snapshot canonical form is not needed here; dedupe by key above.
        demand.available_dependencies.sort();
        buf.push_back(demand);
    }

    /// Drain the buffered demands (host-side; the runtime only records).
    pub fn drain(&self) -> Vec<PortDemand> {
        let mut buf = self.buffer.borrow_mut();
        buf.drain(..).collect()
    }

    /// A snapshot without draining.
    pub fn snapshot(&self) -> Vec<PortDemand> {
        self.buffer.borrow().iter().cloned().collect()
    }

    /// Demands dropped because the buffer was full.
    pub fn dropped(&self) -> u64 {
        *self.dropped.borrow()
    }

    pub fn len(&self) -> usize {
        self.buffer.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.borrow().is_empty()
    }
}

impl Default for DemandSink {
    fn default() -> Self {
        DemandSink::new(DEFAULT_DEMAND_CAP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn demand(surface: &str, gen: &str) -> PortDemand {
        PortDemand {
            requested_surface: surface.to_string(),
            callsite_family: String::from("dispatch"),
            demand_count: 1,
            store_generation: gen.to_string(),
            available_dependencies: vec!["libc:toupper:c-locale:u8:v1".to_string()],
        }
    }

    #[test]
    fn test_sink_aggregates_identical_demands() {
        let sink = DemandSink::new(8);
        sink.record(demand("libc:atoi:v1", "g1"));
        sink.record(demand("libc:atoi:v1", "g1"));
        sink.record(demand("libc:atoi:v1", "g2"));
        let all = sink.snapshot();
        assert_eq!(all.len(), 2, "same surface+generation aggregates");
        let g1 = all.iter().find(|d| d.store_generation == "g1").unwrap();
        assert_eq!(g1.demand_count, 2);
    }

    #[test]
    fn test_sink_is_bounded_and_counts_drops() {
        let sink = DemandSink::new(2);
        for i in 0..5 {
            sink.record(demand(&alloc::format!("libc:f{i}:v1"), "g1"));
        }
        assert_eq!(sink.len(), 2);
        assert_eq!(sink.dropped(), 3);
    }

    #[test]
    fn test_drain_empties_the_buffer() {
        let sink = DemandSink::new(4);
        sink.record(demand("libc:atoi:v1", "g1"));
        assert_eq!(sink.drain().len(), 1);
        assert!(sink.is_empty());
    }

    #[test]
    fn test_canonical_is_order_independent_over_dependencies() {
        let mut a = demand("libc:atoi:v1", "g1");
        a.available_dependencies = vec!["b".to_string(), "a".to_string()];
        let mut b = demand("libc:atoi:v1", "g1");
        b.available_dependencies = vec!["a".to_string(), "b".to_string()];
        assert_eq!(a.canonical(), b.canonical());
    }
}
