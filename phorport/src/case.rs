// phorport/case.rs — the fuzz-input <-> port-case encoding
//
// A fuzz input is an opaque byte string. A *port case* is an ordered list of
// byte-buffer arguments, each in the court ABI form. This module is the single
// place the two are related, so the exploration input space is explicit and the
// design corpus can be injected as a seed corpus (the encoder is the inverse of
// the decoder on every valid case).
//
// The decoder is deliberately **total on a prefix**: a short input yields a
// smaller-but-valid case rather than no case at all, so a coverage-guided
// explorer always gets a signal. Preconditions are still checked afterwards by
// the port's `PortSpec` validator — a decoded case that violates its contract is
// reported `valid: false` and never reaches the foreign oracle.

use phost::porting::target::PortTarget;

/// Exactly `n` bytes starting at `start`, zero-padded when the input is short.
/// Total by construction: the decoder never panics and never indexes past a
/// buffer, so a fuzz worker cannot die inside the harness.
fn window(data: &[u8], start: usize, n: usize) -> Vec<u8> {
    let mut out = vec![0u8; n];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = data.get(start + i).copied().unwrap_or(0);
    }
    out
}

/// Decode a fuzz input into a port case for `target`.
pub fn decode_args(target: &PortTarget, data: &[u8]) -> Option<Vec<Vec<u8>>> {
    let get = |i: usize| data.get(i).copied().unwrap_or(0) as usize;
    match target.symbol {
        "toupper" => Some(vec![vec![get(0) as u8]]),
        "memcmp" => {
            // [n][a: n bytes][b: n bytes]
            let n = get(0).min(8);
            Some(vec![
                window(data, 1, n),
                window(data, 1 + n, n),
                (n as u64).to_le_bytes().to_vec(),
            ])
        }
        "memchr" => {
            // [n][needle][hay: n bytes]
            let n = get(0).min(8);
            let needle = get(1) as u8;
            Some(vec![
                window(data, 2, n),
                vec![needle],
                (n as u64).to_le_bytes().to_vec(),
            ])
        }
        "strlen" => {
            // [n][buf: n bytes] — a missing terminator is repaired, not rejected.
            let n = get(0).max(1).min(8);
            let mut buf = window(data, 1, n);
            if !buf.contains(&0) {
                buf[n - 1] = 0;
            }
            Some(vec![buf, (n as u64).to_le_bytes().to_vec()])
        }
        "strrchr" => {
            // [n][needle][buf: n bytes] — terminator repaired.
            let n = get(0).max(1).min(8);
            let needle = get(1) as u8;
            let mut buf = window(data, 2, n);
            if !buf.contains(&0) {
                buf[n - 1] = 0;
            }
            Some(vec![buf, vec![needle], (n as u64).to_le_bytes().to_vec()])
        }
        "strspn" => {
            // [n][la][accept: la bytes][s: n bytes]. `accept` is NUL-free (a C
            // string set cannot contain NUL); `s` gets a repaired terminator.
            let la = get(1).min(8);
            let n = get(0).max(1).min(8);
            let mut accept = window(data, 2, la);
            for b in accept.iter_mut() {
                if *b == 0 {
                    *b = 0xff;
                }
            }
            let mut s = window(data, 2 + la, n);
            if !s.contains(&0) {
                s[n - 1] = 0;
            }
            Some(vec![s, accept, (n as u64).to_le_bytes().to_vec()])
        }
        _ => None,
    }
}

/// Encode a port case back into a fuzz input (the inverse used for seeds).
pub fn encode_args(target: &PortTarget, args: &[Vec<u8>]) -> Vec<u8> {
    let n = |i: usize| {
        args.get(i)
            .map(|a| phost::porting::candidate::decode_usize(a))
            .unwrap_or(0) as u8
    };
    match target.symbol {
        "toupper" => vec![args.first().and_then(|a| a.first()).copied().unwrap_or(0)],
        "memcmp" => {
            let mut out = vec![n(2)];
            out.extend_from_slice(args.first().map(|a| a.as_slice()).unwrap_or(&[]));
            out.extend_from_slice(args.get(1).map(|a| a.as_slice()).unwrap_or(&[]));
            out
        }
        "memchr" => {
            let mut out = vec![
                n(2),
                args.get(1).and_then(|a| a.first()).copied().unwrap_or(0),
            ];
            out.extend_from_slice(args.first().map(|a| a.as_slice()).unwrap_or(&[]));
            out
        }
        "strlen" => {
            let mut out = vec![n(1)];
            out.extend_from_slice(args.first().map(|a| a.as_slice()).unwrap_or(&[]));
            out
        }
        "strrchr" => {
            let mut out = vec![
                n(2),
                args.get(1).and_then(|a| a.first()).copied().unwrap_or(0),
            ];
            out.extend_from_slice(args.first().map(|a| a.as_slice()).unwrap_or(&[]));
            out
        }
        "strspn" => {
            let accept = args.get(1).map(|a| a.as_slice()).unwrap_or(&[]);
            let mut out = vec![n(2), accept.len() as u8];
            out.extend_from_slice(accept);
            out.extend_from_slice(args.first().map(|a| a.as_slice()).unwrap_or(&[]));
            out
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phost::porting::target::resolve_target;

    /// The decoder is total: no input length, for any registered symbol, may
    /// panic. A fuzz worker must never die inside the harness.
    #[test]
    fn test_decoder_is_total_for_every_symbol_and_length() {
        for sym in ["toupper", "memcmp", "memchr", "strlen", "strrchr", "strspn"] {
            let target = resolve_target(sym).expect("registered");
            for len in 0..24usize {
                let data = vec![0x5au8; len];
                let _ = decode_args(&target, &data);
            }
        }
    }

    #[test]
    fn test_memchr_round_trips() {
        let target = resolve_target("memchr").expect("registered");
        let data = vec![3u8, 0x41, 0x41, 0x42, 0x43];
        let args = decode_args(&target, &data).unwrap();
        assert_eq!(args[0], vec![0x41, 0x42, 0x43]);
        assert_eq!(args[1], vec![0x41]);
        assert_eq!(encode_args(&target, &args), data);
    }

    #[test]
    fn test_strspn_decoder_makes_the_case_valid() {
        let target = resolve_target("strspn").expect("registered");
        // No NUL in the data: the decoder repairs a terminator so the case is in
        // contract; the accept set is made NUL-free.
        let data = vec![3u8, 2, 0x41, 0x00, 0x41, 0x42, 0x43];
        let args = decode_args(&target, &data).unwrap();
        assert!(args[0].contains(&0), "s must have a terminator");
        assert!(!args[1].contains(&0), "accept must be NUL-free");
    }
}
