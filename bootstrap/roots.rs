//! Writes std/koji/roots.kj, the certificate authorities built into the library written in Koji (used
//! when the system has none), from Mozilla's list as curl extracts it (`cacert.pem`, also in the
//! Python package `certifi`):
//!
//!     rustc -O bootstrap/roots.rs -o build/roots && build/roots cacert.pem > std/koji/roots.kj
//!
//! Of each certificate only what a chain is verified with: its subject and its public key (the
//! SubjectPublicKeyInfo), both in DER. The module holds them as one text of base64: for each
//! authority, the size of the subject in 2 bytes (big-endian), the subject, the size of the key in 2
//! bytes, the key.

use std::fmt::Write;

/// The bytes of base64 text (the white space skipped).
fn unbase64(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut bits = 0u32;
    let mut count = 0;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = bits << 6 | value as u32;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    out
}

fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The element of DER at `at`: its tag, where its contents start and where it ends.
fn element(der: &[u8], at: usize) -> (u8, usize, usize) {
    let tag = der[at];
    let first = der[at + 1] as usize;
    if first < 0x80 {
        return (tag, at + 2, at + 2 + first);
    }
    let count = first & 0x7F;
    let mut length = 0;
    for i in 0..count {
        length = length << 8 | der[at + 2 + i] as usize;
    }
    (tag, at + 2 + count, at + 2 + count + length)
}

/// The subject and the SubjectPublicKeyInfo of a certificate, in DER.
fn subject_and_key(der: &[u8]) -> (&[u8], &[u8]) {
    let (_, tbs, _) = element(der, 0);
    let (_, contents, _) = element(der, tbs);
    let mut at = contents;
    // [0] version, then the serial number, the algorithm, the issuer and the validity.
    if der[at] == 0xA0 {
        at = element(der, at).2;
    }
    for _ in 0..4 {
        at = element(der, at).2;
    }
    let subject_end = element(der, at).2;
    let key_end = element(der, subject_end).2;
    (&der[at..subject_end], &der[subject_end..key_end])
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: roots cacert.pem");
    let text = std::fs::read_to_string(&path).expect("cannot read the file");
    let mut blob = Vec::new();
    let mut labels = Vec::new();
    let mut label = String::new();
    let mut pem: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# Label: ") {
            label = rest.trim_matches('"').to_string();
        } else if line == "-----BEGIN CERTIFICATE-----" {
            pem = Some(String::new());
        } else if line == "-----END CERTIFICATE-----" {
            let der = unbase64(&pem.take().unwrap());
            let (subject, key) = subject_and_key(&der);
            for part in [subject, key] {
                blob.extend_from_slice(&(part.len() as u16).to_be_bytes());
                blob.extend_from_slice(part);
            }
            labels.push(std::mem::take(&mut label));
        } else if let Some(body) = pem.as_mut() {
            body.push_str(line);
        }
    }
    let mut out = String::new();
    writeln!(out, "// The certificate authorities of Mozilla's list ({} of them), built into the library written in", labels.len()).unwrap();
    writeln!(out, "// Koji for the systems that have none: written by bootstrap/roots.rs (do not edit). For each, the").unwrap();
    writeln!(out, "// size of its subject in 2 bytes, its subject, the size of its key in 2 bytes and its key").unwrap();
    writeln!(out, "// (SubjectPublicKeyInfo), in DER, all in base64 (see std/koji/x509.kj).").unwrap();
    writeln!(out, "//").unwrap();
    for label in &labels {
        writeln!(out, "// {}", label).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "export immut rootCount = {}", labels.len()).unwrap();
    writeln!(out, "export immut roots = '{}'", base64(&blob)).unwrap();
    print!("{}", out);
}
