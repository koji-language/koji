//! The cryptography of the library written in Koji (std/koji/sha.kj, chacha.kj...), which TLS is made
//! of: a program of C linked with the runtime calls its functions by their names, on the vectors of
//! the standards (FIPS 180-4, RFC 4231, RFC 5869, RFC 8439).

use std::path::Path;
use std::process::Command;

/// The runtime library, as the other tests link it.
const RUNTIME: &str = env!("KOJI_RUNTIME_LIB");

const SYSTEM_LIBRARIES: &[&str] =
    if cfg!(target_os = "macos") { &["-lSystem", "-lc", "-lm"] } else { &["-lpthread", "-ldl", "-lm", "-lrt", "-lutil"] };

/// Builds `source` (C) with the runtime and runs it: what it printed.
fn run_c(name: &str, source: &str) -> String {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("crypto");
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join(format!("{}.c", name));
    let program = directory.join(name);
    std::fs::write(&file, source).unwrap();
    let cc = std::env::var("KOJI_CC").unwrap_or_else(|_| "cc".to_string());
    let built = Command::new(cc).arg("-O2").arg("-o").arg(&program).arg(&file).arg(RUNTIME).args(SYSTEM_LIBRARIES).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let output = Command::new(&program).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

const PRELUDE: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
void koji_init(int argc, const void *argv);
void koji_sha256(const void *, size_t, void *);
void koji_sha384(const void *, size_t, void *);
void koji_sha512(const void *, size_t, void *);
void koji_hmac(int, const void *, size_t, const void *, size_t, void *);
void koji_hkdf(int, const void *, size_t, const void *, size_t, const void *, size_t, size_t, void *);
void koji_chacha20_poly1305_seal(const void *, const void *, const void *, size_t, const void *, size_t, void *);
bool koji_chacha20_poly1305_open(const void *, const void *, const void *, size_t, const void *, size_t, void *);
static void hex(const unsigned char *b, size_t n) { for (size_t i = 0; i < n; i++) printf("%02x", b[i]); printf("\n"); }
"#;

#[test]
fn sha2_hmac_and_hkdf() {
    let source = format!(
        "{}{}",
        PRELUDE,
        r#"
int main(int argc, char **argv) {
    koji_init(argc, argv);
    unsigned char out[64];
    const char *messages[] = {"", "abc", "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"};
    for (int i = 0; i < 3; i++) {
        size_t n = strlen(messages[i]);
        koji_sha256(messages[i], n, out); hex(out, 32);
        koji_sha384(messages[i], n, out); hex(out, 8);
        koji_sha512(messages[i], n, out); hex(out, 8);
    }
    char *million = malloc(1000000);
    memset(million, 'a', 1000000);
    koji_sha256(million, 1000000, out); hex(out, 32);
    koji_sha384(million, 1000000, out); hex(out, 8);
    koji_sha512(million, 1000000, out); hex(out, 8);
    koji_hmac(0, "Jefe", 4, "what do ya want for nothing?", 28, out); hex(out, 32);
    koji_hmac(1, "Jefe", 4, "what do ya want for nothing?", 28, out); hex(out, 48);
    unsigned char ikm[22], salt[13], info[10];
    memset(ikm, 0x0b, 22);
    for (int i = 0; i < 13; i++) salt[i] = (unsigned char)i;
    for (int i = 0; i < 10; i++) info[i] = (unsigned char)(0xf0 + i);
    unsigned char okm[42];
    koji_hkdf(0, salt, 13, ikm, 22, info, 10, 42, okm); hex(okm, 42);
    return 0;
}
"#
    );
    assert_eq!(
        run_c("sha", &source),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n38b060a751ac9638\ncf83e1357eefb8bd\n\
         ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\ncb00753f45a35e8b\nddaf35a193617aba\n\
         248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1\n3391fdddfc8dc739\n204a8fc6dda82f0a\n\
         cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0\n9d0e1809716474cb\ne718483d0ce76964\n\
         5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843\n\
         af45d2e376484031617f78d2b58a6b1b9c7ef464f5a01b47e42ec3736322445e8e2240ca5e69e2c78b3239ecfab21649\n\
         3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865\n"
    );
}

#[test]
fn chacha20_poly1305() {
    let source = format!(
        "{}{}",
        PRELUDE,
        r#"
int main(int argc, char **argv) {
    koji_init(argc, argv);
    unsigned char key[32], nonce[12] = {0x07, 0, 0, 0, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47};
    unsigned char aad[12] = {0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7};
    for (int i = 0; i < 32; i++) key[i] = (unsigned char)(0x80 + i);
    const char *text = "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    size_t n = strlen(text);
    unsigned char sealed[256], opened[256];
    koji_chacha20_poly1305_seal(key, nonce, aad, 12, text, n, sealed);
    hex(sealed, n + 16);
    bool good = koji_chacha20_poly1305_open(key, nonce, aad, 12, sealed, n + 16, opened);
    printf("%d %d\n", good, memcmp(opened, text, n) == 0);
    sealed[n + 3] ^= 1;
    printf("%d\n", koji_chacha20_poly1305_open(key, nonce, aad, 12, sealed, n + 16, opened));
    return 0;
}
"#
    );
    assert_eq!(
        run_c("chacha", &source),
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691\n\
         1 1\n0\n"
    );
}
