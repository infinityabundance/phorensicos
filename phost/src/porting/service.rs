// porting/service.rs — a long-lived sealed native service (std)
//
// The courts derive a seal; the one-shot CLI paths (`port native`, `port compose`)
// each load the committed store for a single call. A running system does not do
// that. This module owns **one verified index** for its whole lifetime, so many
// consumers — leaves and whole chains — are served from the same seal, and every
// leaf object is mapped once and reused across every chain that needs it.
//
// The type is the proof that the store is loaded once: `open` is the only place
// with a path to `store`, and it takes the store path by value. `call` has access
// to nothing but the dispatcher it already holds, so it cannot re-read the store —
// and `session` evidence records `store_loads: 1` alongside the call count.
//
// A session is a deterministic plan: it serves every sealed port in the store
// (five leaves and five compositions) with a fixed argument and a recorded
// expected result, then reports the fan-in — how many consumers each port served.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::porting::dispatch::{DispatchError, DispatchOutcome, DispatchSource, NativeDispatcher};
use crate::porting::store;
use crate::porting::{json_escape, sha256_hex, PortError, PortingAuthority};

/// The store is loaded exactly once, in [`SealedNativeService::open`].
pub const STORE_LOADS_PER_SERVICE: u64 = 1;

/// A long-lived sealed native service: one verified store, many consumers.
pub struct SealedNativeService {
    store_path: String,
    store_residual_hash: String,
    ports_in_store: usize,
    dispatcher: NativeDispatcher,
    calls: u64,
    native_calls: u64,
    fallback_calls: u64,
    broken_seal_calls: u64,
    /// `(port, source, output_hex)` per call, in call order (the session residual).
    log: Vec<(String, &'static str, String)>,
}

impl SealedNativeService {
    /// Open the service.
    ///
    /// Loads and **verifies** the committed store once; the resulting index is
    /// owned by the service. Reading the store is privileged, so this requires
    /// `PORTING` — there is no ambient authority to open the store.
    pub fn open(store_path: &str, auth: &PortingAuthority) -> Result<Self, PortError> {
        if !auth.can_observe() {
            return Err(PortError::CapabilityDenied);
        }
        let (doc, index) =
            store::load_with_document(store_path).map_err(|e| PortError::Io(e.to_string()))?;
        let ports_in_store = index.len();
        Ok(Self {
            store_path: store_path.to_string(),
            store_residual_hash: doc.residual_hash(),
            ports_in_store,
            dispatcher: NativeDispatcher::new(index),
            calls: 0,
            native_calls: 0,
            fallback_calls: 0,
            broken_seal_calls: 0,
            log: Vec::new(),
        })
    }

    /// Open the service over the committed store ([`store::STORE_PATH`]).
    pub fn open_default(auth: &PortingAuthority) -> Result<Self, PortError> {
        Self::open(store::STORE_PATH, auth)
    }

    /// Serve one call by qualified **port id** (a leaf object or a composition).
    ///
    /// No store access: the seal was resolved in [`Self::open`]. A composition is
    /// resolved through the same dispatcher, so its stages are served from the
    /// same verified index.
    pub fn call(
        &mut self,
        port_id: &str,
        args: &[Vec<u8>],
        auth: &PortingAuthority,
    ) -> Result<DispatchOutcome, DispatchError> {
        self.calls += 1;
        let outcome = match self.dispatcher.dispatch_port(port_id, args, auth) {
            Ok(o) => o,
            Err(e) => {
                // A broken seal is terminal and is counted separately from a
                // legitimate foreign fallback.
                self.broken_seal_calls += 1;
                return Err(e);
            }
        };
        match outcome.source {
            DispatchSource::SealedObject => self.native_calls += 1,
            DispatchSource::ForeignFallback => self.fallback_calls += 1,
        }
        self.log.push((
            port_id.to_string(),
            outcome.source.as_str(),
            hex::encode(&outcome.output),
        ));
        Ok(outcome)
    }

    pub fn store_path(&self) -> &str {
        &self.store_path
    }

    pub fn store_residual_hash(&self) -> &str {
        &self.store_residual_hash
    }

    pub fn ports_in_store(&self) -> usize {
        self.ports_in_store
    }

    /// The distinct native objects mapped so far. Reused across every chain, so a
    /// full session over ten ports maps the five leaf objects and no more.
    pub fn objects_mapped(&self) -> usize {
        self.dispatcher.loaded_count()
    }

    pub fn calls(&self) -> u64 {
        self.calls
    }

    pub fn native_calls(&self) -> u64 {
        self.native_calls
    }

    pub fn fallback_calls(&self) -> u64 {
        self.fallback_calls
    }

    pub fn broken_seal_calls(&self) -> u64 {
        self.broken_seal_calls
    }

    /// Every sealed-port resolution in the session, including the stages a chain
    /// dispatches from inside its own runner (not just the top-level calls).
    pub fn dispatches(&self) -> u64 {
        self.dispatcher.dispatches()
    }

    /// Resolutions per port id — the fan-in of one seal into many consumers.
    pub fn per_port_dispatches(&self) -> &BTreeMap<String, u64> {
        self.dispatcher.per_port_dispatches()
    }

    /// Resolutions served by `port_id`.
    pub fn consumers(&self, port_id: &str) -> u64 {
        self.dispatcher
            .per_port_dispatches()
            .get(port_id)
            .copied()
            .unwrap_or(0)
    }

    /// SHA-256 over the session call log — the behavior residual of the session.
    pub fn session_hash(&self) -> String {
        let mut buf = String::new();
        for (i, (port, source, out)) in self.log.iter().enumerate() {
            buf.push_str(&format!("{}|{}|{}|{}\n", i, port, source, out));
        }
        sha256_hex(buf.as_bytes())
    }

    /// The machine-readable session result.
    pub fn verdict(&self, mismatches: &[SessionMismatch]) -> SessionVerdict {
        SessionVerdict {
            store_path: self.store_path.clone(),
            store_residual_hash: self.store_residual_hash.clone(),
            store_loads: STORE_LOADS_PER_SERVICE,
            ports_in_store: self.ports_in_store,
            calls: self.calls,
            native_calls: self.native_calls,
            fallback_calls: self.fallback_calls,
            broken_seal_calls: self.broken_seal_calls,
            objects_mapped: self.objects_mapped(),
            dispatches: self.dispatches(),
            per_port: self.per_port_dispatches().clone(),
            session_hash: self.session_hash(),
            mismatches: mismatches.to_vec(),
        }
    }
}

// ---------------------------------------------------------------------------
// The session plan
// ---------------------------------------------------------------------------

/// One scheduled call: a port, its arguments, and the result it must produce.
#[derive(Clone, Debug)]
pub struct SessionCall {
    pub label: &'static str,
    pub port: &'static str,
    pub args: Vec<Vec<u8>>,
    /// The exact court encoding the sealed port must return.
    pub expect_hex: &'static str,
}

fn n8(n: usize) -> Vec<u8> {
    (n as u64).to_le_bytes().to_vec()
}

/// The deterministic session plan: every sealed port in the store, once.
///
/// The five leaves come first, then the five compositions. Because the
/// compositions resolve to the same leaves, a full session maps **five** objects
/// and serves **ten** ports — that reuse is the point.
pub fn session_plan() -> Vec<SessionCall> {
    alloc::vec![
        // ---- leaves ----
        SessionCall {
            label: "toupper('a')",
            port: crate::porting::target::LIBC_TOUPPER.id,
            args: alloc::vec![alloc::vec![0x61]],
            expect_hex: "41",
        },
        SessionCall {
            label: "memcmp(\"abc\",\"abd\",3)",
            port: crate::porting::target::LIBC_MEMCMP.id,
            args: alloc::vec![
                alloc::vec![0x61, 0x62, 0x63],
                alloc::vec![0x61, 0x62, 0x64],
                n8(3)
            ],
            expect_hex: "ffffffff",
        },
        SessionCall {
            label: "memchr(\"abc\",'b',3)",
            port: crate::porting::target::LIBC_MEMCHR.id,
            args: alloc::vec![alloc::vec![0x61, 0x62, 0x63], alloc::vec![0x62], n8(3)],
            expect_hex: "01000000",
        },
        SessionCall {
            label: "strlen(\"abc\\0\",4)",
            port: crate::porting::target::LIBC_STRLEN.id,
            args: alloc::vec![alloc::vec![0x61, 0x62, 0x63, 0x00], n8(4)],
            expect_hex: "0300000000000000",
        },
        SessionCall {
            label: "strrchr(\"abc\\0\",'b',4)",
            port: crate::porting::target::LIBC_STRRCHR.id,
            args: alloc::vec![
                alloc::vec![0x61, 0x62, 0x63, 0x00],
                alloc::vec![0x62],
                n8(4)
            ],
            expect_hex: "01000000",
        },
        // ---- compositions ----
        SessionCall {
            label: "toupper_memchr(\"aBc\",'b',3)",
            port: crate::porting::composition::COMPOSITION_TOUPPER_MEMCHR.id,
            args: alloc::vec![alloc::vec![0x61, 0x42, 0x63], alloc::vec![0x62], n8(3)],
            expect_hex: "01000000",
        },
        SessionCall {
            label: "toupper_strlen_memchr(\"abc\\0\",'b',4)",
            port: crate::porting::composition_strlen_memchr::COMPOSITION_TOUPPER_STRLEN_MEMCHR.id,
            args: alloc::vec![
                alloc::vec![0x61, 0x62, 0x63, 0x00],
                alloc::vec![0x62],
                n8(4)
            ],
            expect_hex: "01000000",
        },
        SessionCall {
            label: "toupper_strlen_memchr_pair(\"abc\\0\",'b','c',4)",
            port: crate::porting::composition_pair::COMPOSITION_TOUPPER_STRLEN_MEMCHR_PAIR.id,
            args: alloc::vec![
                alloc::vec![0x61, 0x62, 0x63, 0x00],
                alloc::vec![0x62],
                alloc::vec![0x63],
                n8(4)
            ],
            expect_hex: "0100000002000000",
        },
        SessionCall {
            label: "toupper_each(\"abc\",3)",
            port: crate::porting::composition_toupper_each::COMPOSITION_TOUPPER_EACH.id,
            args: alloc::vec![alloc::vec![0x61, 0x62, 0x63], n8(3)],
            expect_hex: "414243",
        },
        SessionCall {
            label: "toupper_each_strlen_memchr(\"abc\\0\",'b',4)",
            port: crate::porting::composition_nested::COMPOSITION_TOUPPER_EACH_STRLEN_MEMCHR.id,
            args: alloc::vec![
                alloc::vec![0x61, 0x62, 0x63, 0x00],
                alloc::vec![0x62],
                n8(4)
            ],
            expect_hex: "01000000",
        },
    ]
}

/// One planned call whose actual result did not match the recorded expectation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionMismatch {
    pub label: String,
    pub port: String,
    pub expected_hex: String,
    pub actual_hex: String,
    pub reason: String,
}

/// Run the deterministic session plan through **one** service.
///
/// Returns the verdict plus every mismatch. The store is loaded once; every port
/// is served from the same seal.
pub fn run_session(
    store_path: &str,
    auth: &PortingAuthority,
) -> Result<(SessionVerdict, Vec<SessionMismatch>, SealedNativeService), PortError> {
    let mut service = SealedNativeService::open(store_path, auth)?;
    let mut mismatches: Vec<SessionMismatch> = Vec::new();

    for call in session_plan() {
        match service.call(call.port, &call.args, auth) {
            Ok(o) if o.source.is_native() => {
                let actual = hex::encode(&o.output);
                if actual != call.expect_hex {
                    mismatches.push(SessionMismatch {
                        label: call.label.to_string(),
                        port: call.port.to_string(),
                        expected_hex: call.expect_hex.to_string(),
                        actual_hex: actual,
                        reason: String::from("sealed output differs from the recorded expectation"),
                    });
                }
            }
            Ok(_) => mismatches.push(SessionMismatch {
                label: call.label.to_string(),
                port: call.port.to_string(),
                expected_hex: call.expect_hex.to_string(),
                actual_hex: String::from("!foreign fallback"),
                reason: String::from("port was not served by a sealed object"),
            }),
            Err(e) => mismatches.push(SessionMismatch {
                label: call.label.to_string(),
                port: call.port.to_string(),
                expected_hex: call.expect_hex.to_string(),
                actual_hex: String::from("!broken seal"),
                reason: format!("{}", e),
            }),
        }
    }

    let verdict = service.verdict(&mismatches);
    Ok((verdict, mismatches, service))
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

/// The session residual: many consumers served from one verified store load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionVerdict {
    pub store_path: String,
    pub store_residual_hash: String,
    /// Exactly one load per service (`open`); `call` cannot touch the store.
    pub store_loads: u64,
    pub ports_in_store: usize,
    pub calls: u64,
    pub native_calls: u64,
    pub fallback_calls: u64,
    pub broken_seal_calls: u64,
    pub objects_mapped: usize,
    /// Every sealed-port resolution, including the stages a chain dispatches.
    pub dispatches: u64,
    /// Resolutions per port id — the fan-in of one seal into many consumers.
    pub per_port: BTreeMap<String, u64>,
    pub session_hash: String,
    pub mismatches: Vec<SessionMismatch>,
}

impl SessionVerdict {
    pub fn target(&self) -> &'static str {
        "phor:session:sealed-native-service:v1"
    }

    /// Consistent only when every planned call was served natively, nothing fell
    /// back or broke, and every recorded expectation matched exactly.
    pub fn is_consistent(&self) -> bool {
        self.calls > 0
            && self.calls == self.native_calls
            && self.fallback_calls == 0
            && self.broken_seal_calls == 0
            && self.mismatches.is_empty()
    }

    pub fn verdict_str(&self) -> &'static str {
        if self.is_consistent() {
            "consistent"
        } else {
            "inconsistent"
        }
    }

    pub fn canonical(&self) -> String {
        let per_port: Vec<String> = self
            .per_port
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        format!(
            "target={};store={};store_residual_hash={};store_loads={};ports_in_store={};calls={};native_calls={};fallback_calls={};broken_seal_calls={};objects_mapped={};dispatches={};per_port={};session_hash={};verdict={}",
            self.target(),
            self.store_path,
            self.store_residual_hash,
            self.store_loads,
            self.ports_in_store,
            self.calls,
            self.native_calls,
            self.fallback_calls,
            self.broken_seal_calls,
            self.objects_mapped,
            self.dispatches,
            per_port.join(","),
            self.session_hash,
            self.verdict_str()
        )
    }

    pub fn residual_hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn to_json(&self) -> String {
        let per_port: Vec<String> = self
            .per_port
            .iter()
            .map(|(k, v)| format!("\n      \"{}\": {}", json_escape(k), v))
            .collect();
        let body: Vec<String> = self
            .mismatches
            .iter()
            .map(|m| {
                format!(
                    "    {{\n      \"label\": \"{}\",\n      \"port\": \"{}\",\n      \"expected_hex\": \"{}\",\n      \"actual_hex\": \"{}\",\n      \"reason\": \"{}\"\n    }}",
                    json_escape(&m.label),
                    json_escape(&m.port),
                    json_escape(&m.expected_hex),
                    json_escape(&m.actual_hex),
                    json_escape(&m.reason)
                )
            })
            .collect();
        format!(
            "{{\n  \"schema\": \"phorensic.porting.session_verdict.v1\",\n  \"target\": \"{}\",\n  \"store\": \"{}\",\n  \"store_residual_hash\": \"{}\",\n  \"store_loads\": {},\n  \"ports_in_store\": {},\n  \"calls\": {},\n  \"native_calls\": {},\n  \"fallback_calls\": {},\n  \"broken_seal_calls\": {},\n  \"objects_mapped\": {},\n  \"dispatches\": {},\n  \"per_port\": {{{}\n  }},\n  \"session_hash\": \"{}\",\n  \"verdict\": \"{}\",\n  \"mismatches\": [\n{}\n  ],\n  \"residual_hash\": \"{}\"\n}}\n",
            json_escape(self.target()),
            json_escape(&self.store_path),
            self.store_residual_hash,
            self.store_loads,
            self.ports_in_store,
            self.calls,
            self.native_calls,
            self.fallback_calls,
            self.broken_seal_calls,
            self.objects_mapped,
            self.dispatches,
            per_port.join(","),
            self.session_hash,
            self.verdict_str(),
            body.join(",\n"),
            self.residual_hash()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porting::target::{LIBC_MEMCHR, LIBC_TOUPPER};

    fn granted() -> PortingAuthority {
        PortingAuthority::granted()
    }

    #[test]
    fn test_session_plan_covers_every_sealed_port() {
        let plan = session_plan();
        let ports: Vec<&str> = plan.iter().map(|c| c.port).collect();
        assert_eq!(ports.len(), 10, "the plan serves every sealed port once");
        assert_eq!(
            ports.iter().filter(|p| p.starts_with("libc:")).count(),
            5,
            "five leaves"
        );
        assert_eq!(
            ports
                .iter()
                .filter(|p| p.starts_with("phor:compose:"))
                .count(),
            5,
            "five compositions"
        );
        for c in &plan {
            assert!(!c.args.is_empty(), "{} has arguments", c.label);
            assert_eq!(c.expect_hex.len() % 2, 0, "{} expects whole bytes", c.label);
        }
    }

    #[test]
    fn test_service_denies_without_capability() {
        match SealedNativeService::open_default(&PortingAuthority::none()) {
            Err(PortError::CapabilityDenied) => {}
            Err(e) => panic!("expected CapabilityDenied, got {:?}", e),
            Ok(_) => panic!("the service opened without the PORTING capability"),
        }
    }

    #[test]
    fn test_session_is_consistent_and_serves_ten_ports_from_five_objects() {
        let (v, mismatches, service) = run_session(store::STORE_PATH, &granted()).unwrap();
        assert!(mismatches.is_empty(), "{:?}", mismatches);
        assert!(v.is_consistent());
        assert_eq!(v.store_loads, 1);
        assert_eq!(v.calls, 10);
        assert_eq!(v.native_calls, 10);
        assert_eq!(v.fallback_calls, 0);
        assert_eq!(v.broken_seal_calls, 0);
        assert_eq!(v.ports_in_store, 10);
        // Ten ports, five objects: the leaves are mapped once and reused by every
        // chain that consumes them.
        assert_eq!(v.objects_mapped, 5);
        assert_eq!(service.objects_mapped(), 5);
        // Ten top-level calls, but many more resolutions once a chain's own stages
        // are counted.
        assert_eq!(v.dispatches, service.dispatches());
        assert!(v.dispatches > v.calls, "dispatches were {}", v.dispatches);
    }

    #[test]
    fn test_session_shows_fan_in_of_shared_ports() {
        let (v, _, _) = run_session(store::STORE_PATH, &granted()).unwrap();
        // `toupper` is consumed by its own leaf call and by every map stage of
        // every chain, so its resolution count is far above one.
        let toupper = v.per_port.get(LIBC_TOUPPER.id).copied().unwrap_or(0);
        assert!(toupper > 1, "toupper fan-in was {}", toupper);
        // `memchr` is consumed by its own leaf call and by the search stage of
        // four chains.
        let memchr = v.per_port.get(LIBC_MEMCHR.id).copied().unwrap_or(0);
        assert!(memchr >= 4, "memchr fan-in was {}", memchr);
        // Every composition was resolved as a port of its own.
        for kind in [
            crate::porting::CompositionKind::ToupperMemchr,
            crate::porting::CompositionKind::ToupperStrlenMemchr,
            crate::porting::CompositionKind::ToupperStrlenMemchrPair,
            crate::porting::CompositionKind::ToupperEach,
            crate::porting::CompositionKind::ToupperEachStrlenMemchr,
        ] {
            let n = v.per_port.get(kind.target_id()).copied().unwrap_or(0);
            assert!(n >= 1, "{} was not served by the service", kind.target_id());
        }
        // The nested chain consumes the sealed **composition** `toupper_each` twice
        // (fold the haystack, fold the needle) on top of its own top-level call — a
        // composition consumed by another composition, from the same index.
        let each = v
            .per_port
            .get(crate::porting::CompositionKind::ToupperEach.target_id())
            .copied()
            .unwrap_or(0);
        assert!(each >= 3, "toupper_each resolutions were {}", each);
    }

    #[test]
    fn test_session_is_deterministic() {
        let (a, _, _) = run_session(store::STORE_PATH, &granted()).unwrap();
        let (b, _, _) = run_session(store::STORE_PATH, &granted()).unwrap();
        assert_eq!(a.session_hash, b.session_hash);
        assert_eq!(a.residual_hash(), b.residual_hash());
        assert_eq!(a.to_json(), b.to_json());
    }

    #[test]
    fn test_service_call_reads_no_store_after_open() {
        // Copy the committed index to a temp path, open from it, then delete it.
        // A call that re-read the store would fail; the service does not.
        let root = crate::porting::compiled::workspace_root();
        let tmp =
            std::env::temp_dir().join(format!("phost_store_service_{}.json", std::process::id()));
        std::fs::copy(root.join(store::STORE_PATH), &tmp).unwrap();
        let mut service = SealedNativeService::open(tmp.to_str().unwrap(), &granted()).unwrap();
        std::fs::remove_file(&tmp).unwrap();

        let out = service
            .call(LIBC_TOUPPER.id, &alloc::vec![alloc::vec![0x61]], &granted())
            .unwrap();
        assert!(out.source.is_native());
        assert_eq!(hex::encode(&out.output), "41");
        // A composition too: it resolves its stages from the index already held.
        let out = service
            .call(
                crate::porting::composition::COMPOSITION_TOUPPER_MEMCHR.id,
                &alloc::vec![alloc::vec![0x61, 0x42, 0x63], alloc::vec![0x62], n8(3)],
                &granted(),
            )
            .unwrap();
        assert_eq!(hex::encode(&out.output), "01000000");
        assert_eq!(service.objects_mapped(), 2);
        // 1 leaf call + (1 composition resolution + 3 hay folds + 1 needle fold
        // + 1 search) = 7 sealed-port resolutions, all from the index already held.
        assert_eq!(service.dispatches(), 7);
    }

    #[test]
    fn test_service_without_a_store_fails_closed() {
        let err = SealedNativeService::open("phost/evidence/store/nope.json", &granted());
        assert!(matches!(err, Err(PortError::Io(_))));
    }

    #[test]
    fn test_session_hash_changes_when_the_log_changes() {
        let (a, _, _) = run_session(store::STORE_PATH, &granted()).unwrap();
        let mut b = a.clone();
        b.session_hash = "0".repeat(64);
        assert_ne!(a.residual_hash(), b.residual_hash());
    }
}
