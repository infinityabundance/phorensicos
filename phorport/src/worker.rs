// phorport/worker.rs — the untrusted candidate execution worker (§13)
//
// Phase 7.5. Once candidates are machine generated, the coordinator must not
// hand arbitrary emitted machine code directly to a long-lived process and call
// it. This module runs candidate execution in a **separate, bounded process**:
//
//   * the coordinator sends one load request (target + object + hash) and then
//     one request per case, over a length-prefixed protocol;
//   * the worker verifies the object with the *authority's own loader*
//     (`SealedObjectHandle::load`: hash, ELF64 format, x86-64 architecture, the
//     declared entry symbol, no relocations, load bounds) before executing;
//   * the child runs with `no_new_privs`, `RLIMIT_AS`, `RLIMIT_CPU`,
//     `RLIMIT_CORE=0`, `RLIMIT_NOFILE`, a cleared environment and piped stdio;
//   * a crash, `SIGILL`, abort, timeout or resource exhaustion becomes a
//     deterministic per-case outcome and never kills the coordinator;
//   * the worker is persistent (one process, many cases) and is restarted
//     transparently after a crash, so throughput is preserved without trusting
//     the child.
//
// The comparison itself is unchanged: the worker returns raw candidate bytes and
// `harness::probe_validated` applies the same normalization and residual
// classification as the in-process path.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use phost::porting::exec::SealedObjectHandle;
use phost::porting::target::{resolve_target, PortTarget};

use crate::config::HarnessConfig;
use crate::harness::CandidateExec;

/// The default address-space limit for the worker (256 MiB).
pub const DEFAULT_MEMORY_LIMIT: u64 = 256 * 1024 * 1024;
/// The default per-call wall-clock timeout.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// The worker's cumulative CPU limit, in seconds.
pub const WORKER_CPU_SECONDS: u64 = 60;

// ---------------------------------------------------------------------------
// Protocol frames
// ---------------------------------------------------------------------------

const KIND_LOAD: u8 = 0x01;
const KIND_CASE: u8 = 0x02;
const REPLY_LOAD: u8 = 0x81;
const REPLY_CASE: u8 = 0x82;

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let b = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn u32(&mut self) -> Option<u32> {
        let s = self.buf.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_le_bytes(s.try_into().ok()?))
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        let s = self.buf.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }

    fn string(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?.to_vec()).ok()
    }

    fn done(&self) -> bool {
        self.pos == self.buf.len()
    }
}

// ---------------------------------------------------------------------------
// The worker side (the child)
// ---------------------------------------------------------------------------

/// Read one length-prefixed frame from `r`.
fn read_frame<R: Read>(r: &mut R) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n > 64 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Write one length-prefixed frame to `w`.
fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> std::io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Run the worker loop. This is the hidden `phorport __worker` subcommand.
///
/// Returns the process exit code.
pub fn run_worker() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    // The currently loaded object, if any.
    let mut loaded: Option<(PortTarget, SealedObjectHandle)> = None;

    loop {
        let frame = match read_frame(&mut input) {
            Ok(f) => f,
            Err(_) => return 0, // coordinator closed the pipe
        };
        let mut r = Reader::new(&frame);
        match r.u8() {
            Some(KIND_LOAD) => {
                let target_id = r.string().unwrap_or_default();
                let path = r.string().unwrap_or_default();
                let hash = r.string().unwrap_or_default();
                let target = match resolve_target(&target_id) {
                    Some(t) => t,
                    None => {
                        let mut out = vec![REPLY_LOAD, 1];
                        put_str(&mut out, "unknown target");
                        let _ = write_frame(&mut output, &out);
                        continue;
                    }
                };
                // The authority's own loader performs every ELF check.
                match SealedObjectHandle::load(&target, &path, &hash) {
                    Ok(handle) => {
                        loaded = Some((target, handle));
                        let _ = write_frame(&mut output, &[REPLY_LOAD, 0]);
                    }
                    Err(e) => {
                        let mut out = vec![REPLY_LOAD, 1];
                        put_str(&mut out, &e.as_str());
                        let _ = write_frame(&mut output, &out);
                    }
                }
            }
            Some(KIND_CASE) => {
                let argc = r.u32().unwrap_or(0) as usize;
                let mut args: Vec<Vec<u8>> = Vec::with_capacity(argc);
                let mut ok = true;
                for _ in 0..argc {
                    match r.bytes() {
                        Some(b) => args.push(b.to_vec()),
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok || !r.done() {
                    let mut out = vec![REPLY_CASE, 1];
                    put_str(&mut out, "malformed case frame");
                    let _ = write_frame(&mut output, &out);
                    continue;
                }
                match &loaded {
                    Some((target, handle)) => match handle.call(target, &args) {
                        Ok(bytes) => {
                            let mut out = vec![REPLY_CASE, 0];
                            put_bytes(&mut out, &bytes);
                            let _ = write_frame(&mut output, &out);
                        }
                        Err(e) => {
                            let mut out = vec![REPLY_CASE, 1];
                            put_str(&mut out, &e.as_str());
                            let _ = write_frame(&mut output, &out);
                        }
                    },
                    None => {
                        let mut out = vec![REPLY_CASE, 1];
                        put_str(&mut out, "no object loaded");
                        let _ = write_frame(&mut output, &out);
                    }
                }
            }
            _ => {
                // Unknown frame kind: fail the connection closed.
                return 2;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The coordinator side (the parent)
// ---------------------------------------------------------------------------

/// A deterministic classification of how a candidate case failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateFailure {
    /// The object failed the authority's ELF/hash verification.
    Load(String),
    /// The candidate's own entry returned an execution error.
    Exec(String),
    /// The worker died (signal or non-zero exit) while executing the case.
    Crashed(i32),
    /// The worker did not answer within the call timeout.
    Timeout,
    /// The containment machinery itself failed (spawn, protocol).
    Containment(String),
}

impl CandidateFailure {
    /// A stable, leak-free description (used as the residual reason).
    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateFailure::Load(_) => "candidate-object-rejected",
            CandidateFailure::Exec(_) => "candidate-execution-error",
            CandidateFailure::Crashed(_) => "candidate-crashed",
            CandidateFailure::Timeout => "candidate-timeout",
            CandidateFailure::Containment(_) => "containment-error",
        }
    }

    /// The detail (never echoes candidate output; the loader/exec messages are
    /// the authority's own, and are bounded).
    pub fn detail(&self) -> String {
        match self {
            CandidateFailure::Load(m) | CandidateFailure::Exec(m) => m.clone(),
            CandidateFailure::Crashed(sig) => format!("worker exited with signal/status {sig}"),
            CandidateFailure::Timeout => String::from("worker exceeded the call timeout"),
            CandidateFailure::Containment(m) => m.clone(),
        }
    }
}

struct WorkerState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    rx: Option<Receiver<Vec<u8>>>,
}

/// A contained candidate executor: one persistable worker process.
pub struct ContainedExec {
    exe: PathBuf,
    memory_limit: u64,
    call_timeout: Duration,
    state: RefCell<WorkerState>,
}

impl ContainedExec {
    /// Build a contained executor for `exe` (usually `std::env::current_exe()`).
    pub fn new(exe: PathBuf) -> Self {
        ContainedExec {
            exe,
            memory_limit: DEFAULT_MEMORY_LIMIT,
            call_timeout: DEFAULT_CALL_TIMEOUT,
            state: RefCell::new(WorkerState {
                child: None,
                stdin: None,
                rx: None,
            }),
        }
    }

    /// Override the address-space limit.
    pub fn with_memory_limit(mut self, bytes: u64) -> Self {
        self.memory_limit = bytes;
        self
    }

    /// Override the per-call wall-clock timeout.
    pub fn with_call_timeout(mut self, t: Duration) -> Self {
        self.call_timeout = t;
        self
    }

    /// Spawn the worker and load `config`'s object. Returns a load failure
    /// deterministically.
    fn ensure_loaded(
        &self,
        config: &HarnessConfig,
        target: &PortTarget,
    ) -> Result<(), CandidateFailure> {
        let mut st = self.state.borrow_mut();
        if st.child.is_some() {
            return Ok(());
        }
        self.spawn(&mut st)?;
        // Load request.
        let mut payload = vec![KIND_LOAD];
        put_str(&mut payload, &config.target_id);
        put_str(&mut payload, &config.candidate_path);
        put_str(&mut payload, &config.candidate_hash);
        let reply = self.exchange(&mut st, payload)?;
        let mut r = Reader::new(&reply);
        match (r.u8(), r.u8()) {
            (Some(REPLY_LOAD), Some(0)) => Ok(()),
            (Some(REPLY_LOAD), Some(_)) => {
                let msg = r.string().unwrap_or_else(|| String::from("rejected"));
                let _ = self.reset(&mut st);
                let _ = target;
                Err(CandidateFailure::Load(msg))
            }
            _ => {
                let _ = self.reset(&mut st);
                Err(CandidateFailure::Containment(String::from(
                    "malformed load reply",
                )))
            }
        }
    }

    fn spawn(&self, st: &mut WorkerState) -> Result<(), CandidateFailure> {
        let (child, stdin, rx) = spawn_worker(&self.exe, self.memory_limit)
            .map_err(|e| CandidateFailure::Containment(e.to_string()))?;
        st.child = Some(child);
        st.stdin = Some(stdin);
        st.rx = Some(rx);
        Ok(())
    }

    fn reset(&self, st: &mut WorkerState) -> Option<i32> {
        let status = st.child.take().map(|mut c| {
            let _ = c.kill();
            c.wait().ok().and_then(|s| s.code())
        });
        st.stdin = None;
        st.rx = None;
        status.flatten()
    }

    /// Send a request and receive one reply, with a wall-clock bound.
    fn exchange(
        &self,
        st: &mut WorkerState,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, CandidateFailure> {
        let stdin = st
            .stdin
            .as_mut()
            .ok_or_else(|| CandidateFailure::Containment(String::from("no worker stdin")))?;
        if let Err(e) = write_frame(stdin, &payload) {
            let _ = e;
            self.reset(st);
            return Err(CandidateFailure::Crashed(-1));
        }
        let rx = st
            .rx
            .as_ref()
            .ok_or_else(|| CandidateFailure::Containment(String::from("no worker channel")))?;
        match rx.recv_timeout(self.call_timeout) {
            Ok(frame) => Ok(frame),
            Err(RecvTimeoutError::Timeout) => {
                let _ = self.reset(st);
                Err(CandidateFailure::Timeout)
            }
            Err(RecvTimeoutError::Disconnected) => {
                // The reader saw EOF: the worker died.
                let sig = st
                    .child
                    .as_mut()
                    .and_then(|c| c.wait().ok())
                    .and_then(|s| s.code())
                    .unwrap_or(-1);
                st.child = None;
                st.stdin = None;
                st.rx = None;
                Err(CandidateFailure::Crashed(sig))
            }
        }
    }
}

impl CandidateExec for ContainedExec {
    fn call(
        &self,
        config: &HarnessConfig,
        target: &PortTarget,
        args: &[Vec<u8>],
    ) -> Result<Vec<u8>, String> {
        if let Err(f) = self.ensure_loaded(config, target) {
            return Err(format!("{}: {}", f.as_str(), f.detail()));
        }
        let mut st = self.state.borrow_mut();
        let mut payload = vec![KIND_CASE];
        payload.extend_from_slice(&(args.len() as u32).to_le_bytes());
        for a in args {
            put_bytes(&mut payload, a);
        }
        let reply = match self.exchange(&mut st, payload) {
            Ok(r) => r,
            Err(f) => return Err(format!("{}: {}", f.as_str(), f.detail())),
        };
        let mut r = Reader::new(&reply);
        match (r.u8(), r.u8()) {
            (Some(REPLY_CASE), Some(0)) => Ok(r.bytes().unwrap_or_default().to_vec()),
            (Some(REPLY_CASE), Some(_)) => {
                let msg = r
                    .string()
                    .unwrap_or_else(|| String::from("execution error"));
                Err(format!(
                    "{}: {}",
                    CandidateFailure::Exec(String::new()).as_str(),
                    msg
                ))
            }
            _ => {
                let _ = self.reset(&mut st);
                Err(format!(
                    "{}: malformed case reply",
                    CandidateFailure::Containment(String::new()).as_str()
                ))
            }
        }
    }
}

/// Spawn a contained worker: a thread pumps its stdout frames into a channel.
fn spawn_worker(
    exe: &Path,
    memory_limit: u64,
) -> std::io::Result<(Child, ChildStdin, Receiver<Vec<u8>>)> {
    let mut cmd = Command::new(exe);
    cmd.arg("__worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_clear();
    contain(&mut cmd, memory_limit);

    let mut child = cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no child stdin"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no child stdout"))?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || loop {
        match read_frame(&mut stdout) {
            Ok(frame) => {
                if tx.send(frame).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    });
    Ok((child, stdin, rx))
}

/// Apply the containment controls to a child command.
#[cfg(unix)]
fn contain(cmd: &mut Command, memory_limit: u64) {
    use std::os::unix::process::CommandExt;
    // SAFETY: every call is async-signal-safe and touches only per-process
    // limits/privileges of the forked child before exec.
    unsafe {
        cmd.pre_exec(move || {
            // No new privileges: the child cannot gain capabilities.
            libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
            set_limit(libc::RLIMIT_AS, memory_limit);
            set_limit(libc::RLIMIT_CPU, WORKER_CPU_SECONDS);
            set_limit(libc::RLIMIT_CORE, 0);
            set_limit(libc::RLIMIT_NOFILE, 64);
            Ok(())
        });
    }
}

#[cfg(unix)]
fn set_limit(resource: libc::__rlimit_resource_t, value: u64) {
    let lim = libc::rlimit {
        rlim_cur: value as libc::rlim_t,
        rlim_max: value as libc::rlim_t,
    };
    unsafe {
        libc::setrlimit(resource, &lim);
    }
}

#[cfg(not(unix))]
fn contain(_cmd: &mut Command, _memory_limit: u64) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{probe_args, probe_args_with};
    use phost::porting::target::resolve_target;

    fn phorc_path() {
        let debug_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", debug_dir.display()));
    }

    /// The `phorport` binary (the worker's host process). `cargo test` builds bin
    /// targets, so this is present; fail with a clear instruction otherwise.
    fn phorport_exe() -> PathBuf {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/debug/phorport");
        assert!(
            p.is_file(),
            "the phorport binary is required for the containment tests; build it with `cargo build -p phorport` ({})",
            p.display()
        );
        p
    }

    fn compile_candidate(symbol: &str, source: &str, tag: &str) -> (HarnessConfig, PortTarget) {
        phorc_path();
        let target = resolve_target(symbol).expect("target");
        let work =
            std::env::temp_dir().join(format!("phorport-worker-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        let id = crate::compile::compile_source(symbol, source, &work).expect("compile");
        (
            HarnessConfig {
                target_id: target.id.to_string(),
                candidate_path: id.object_path,
                candidate_hash: id.object_hash,
            },
            target,
        )
    }

    fn correct_strspn() -> String {
        std::fs::read_to_string("../examples/jit_port_strspn.phor")
            .or_else(|_| std::fs::read_to_string("examples/jit_port_strspn.phor"))
            .expect("source")
    }

    #[test]
    fn test_contained_execution_agrees_with_the_in_process_court() {
        let (config, target) = compile_candidate("strspn", &correct_strspn(), "agree");
        let exec = ContainedExec::new(phorport_exe());
        let cases: Vec<Vec<Vec<u8>>> = vec![
            vec![
                b"abc\0".to_vec(),
                b"abc".to_vec(),
                4u64.to_le_bytes().to_vec(),
            ],
            vec![
                b"xyz\0".to_vec(),
                b"ab".to_vec(),
                4u64.to_le_bytes().to_vec(),
            ],
            vec![b"\0".to_vec(), b"a".to_vec(), 1u64.to_le_bytes().to_vec()],
        ];
        for args in &cases {
            let inproc = probe_args(&config, args);
            let contained = probe_args_with(&exec, &config, args);
            assert_eq!(inproc.matched, contained.matched, "args {args:?}");
            assert_eq!(inproc.oracle_hex, contained.oracle_hex);
            assert_eq!(inproc.candidate_hex, contained.candidate_hex);
            let _ = &target;
        }
    }

    #[test]
    fn test_a_rejected_object_is_a_deterministic_load_failure() {
        let (mut config, target) = compile_candidate("strspn", &correct_strspn(), "loadrej");
        // Point the hash at the real object but corrupt the bytes on disk: the
        // loader's hash check must reject it, and the coordinator must survive.
        std::fs::write(&config.candidate_path, b"not an elf object").expect("corrupt");
        let exec = ContainedExec::new(phorport_exe());
        let args = vec![
            b"abc\0".to_vec(),
            b"abc".to_vec(),
            4u64.to_le_bytes().to_vec(),
        ];
        let out = exec.call(&config, &target, &args);
        assert!(out.is_err(), "a corrupted object must not execute");
        let msg = out.unwrap_err();
        assert!(msg.contains("candidate") || msg.contains("object"), "{msg}");
        // The coordinator is still alive and can run a fresh worker afterwards.
        let (good, target2) = compile_candidate("strspn", &correct_strspn(), "loadrej2");
        let exec2 = ContainedExec::new(phorport_exe());
        let out2 = exec2.call(&good, &target2, &args);
        assert!(out2.is_ok(), "{out2:?}");
        let _ = config;
    }

    #[test]
    fn test_a_killed_worker_is_transparently_restarted() {
        let (config, target) = compile_candidate("strspn", &correct_strspn(), "restart");
        let exec = ContainedExec::new(phorport_exe());
        let args = vec![
            b"abc\0".to_vec(),
            b"abc".to_vec(),
            4u64.to_le_bytes().to_vec(),
        ];
        // First call spawns the worker.
        assert!(exec.call(&config, &target, &args).is_ok());
        // Kill the worker behind the coordinator's back.
        {
            let mut st = exec.state.borrow_mut();
            if let Some(child) = st.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        // The next call must observe the crash deterministically (or restart and
        // succeed); either way the coordinator survives and ends consistent.
        let mut consistent = false;
        for _ in 0..2 {
            if exec.call(&config, &target, &args).is_ok() {
                consistent = true;
                break;
            }
        }
        assert!(
            consistent,
            "the contained executor did not recover from a crash"
        );
    }
}
