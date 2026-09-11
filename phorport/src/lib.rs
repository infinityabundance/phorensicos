// phorport — the host-only autonomous porting foundry
//
// Phase 4 of docs/AUTONOMOUS_PORTING_ARCHITECTURE.md. This crate is the one
// host-side orchestration crate: it is deliberately *outside* the `phost` /
// `phost_kernel` dependency closure, so nothing here (and nothing it pulls in)
// can enter the sealed runtime.
//
// Its job in this phase is the counterexample engine's harness and bookkeeping:
//
//   * `case`           — the fuzz-input <-> port-case encoding (and its inverse,
//                        so the design corpus can seed exploration);
//   * `harness`        — the differential comparison: precondition gate, foreign
//                        oracle, compiled `.phor` object execution, normalized
//                        comparison, structural residual;
//   * `residual`       — the Phorensicos structural residual bank (`PORT.*`);
//   * `minimize`       — court-verified minimization (the minimizer proposes, the
//                        comparison court decides);
//   * `counterexample` — durable, content-addressed counterexample records;
//   * `explore`        — the FRF-Fuzz bridge: generate a differential target,
//                        seed it with the design corpus, run a bounded campaign,
//                        turn findings into minimized counterexamples.

pub mod ablation;
pub mod case;
pub mod cegis;
pub mod compile;
pub mod config;
pub mod counterexample;
pub mod demand;
pub mod explore;
pub mod frf;
pub mod generations;
pub mod harness;
pub mod memory;
pub mod minimize;
pub mod multioracle;
pub mod pipeline;
pub mod producer;
pub mod qualification;
pub mod residual;
pub mod synth;
pub mod worker;
