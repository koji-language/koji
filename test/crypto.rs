//! The cryptography of the library written in Koji (std/koji/sha.kj, chacha.kj, x25519.kj, bignum.kj,
//! ecc.kj, rsa.kj), which TLS is made of: a program of C linked with the runtime calls its functions by
//! their names, on the vectors of the standards (FIPS 180-4, RFC 4231, RFC 5869, RFC 8439, RFC 7748)
//! and on keys and signatures made by OpenSSL (P-256, P-384, RSA).

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
void koji_x25519(void *, const void *, const void *);
static void unhex(const char *h, unsigned char *b, size_t n) { for (size_t i = 0; i < n; i++) sscanf(h + 2 * i, "%2hhx", &b[i]); }
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

#[test]
fn x25519() {
    let source = format!(
        "{}{}",
        PRELUDE,
        r#"
int main(int argc, char **argv) {
    koji_init(argc, argv);
    unsigned char scalar[32], point[32], out[32], alice[32], bob[32], base[32] = {9};
    unhex("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4", scalar, 32);
    unhex("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c", point, 32);
    koji_x25519(out, scalar, point); hex(out, 32);
    unhex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a", alice, 32);
    unhex("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb", bob, 32);
    unsigned char alicePublic[32], bobPublic[32];
    koji_x25519(alicePublic, alice, base); hex(alicePublic, 32);
    koji_x25519(bobPublic, bob, base); hex(bobPublic, 32);
    koji_x25519(out, alice, bobPublic); hex(out, 32);
    koji_x25519(out, bob, alicePublic); hex(out, 32);
    return 0;
}
"#
    );
    assert_eq!(
        run_c("x25519", &source),
        "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552\n\
         8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a\n\
         de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f\n\
         4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742\n\
         4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742\n"
    );
}

#[test]
fn bignum_ecc_and_rsa() {
    let source = format!(
        "{}{}",
        PRELUDE,
        r#"
void koji_modular_power(const void *, const void *, size_t, const void *, size_t, void *);
void koji_ecdh_public(int, const void *, void *);
bool koji_ecdh_shared(int, const void *, const void *, size_t, void *);
bool koji_ecdsa_verify(int, const void *, size_t, const void *, const void *, const void *, size_t);
bool koji_rsa_verify(int, int, const void *, size_t, const void *, size_t, const void *, size_t, const void *, size_t);
static size_t load(const char *h, unsigned char *b) { size_t n = strlen(h) / 2; unhex(h, b, n); return n; }
static void curve(int which, size_t size, const char *secret, const char *theirs, const char *digest, const char *r, const char *s) {
    unsigned char k[48], q[97], q2[97], out[97], d[48], rb[48], sb[48];
    load(secret, k); load(theirs, q); load(digest, d); load(r, rb); load(s, sb);
    koji_ecdh_public(which, k, out); hex(out, 1 + 2 * size);
    printf("%d ", koji_ecdh_shared(which, k, q, 1 + 2 * size, out)); hex(out, size);
    memcpy(q2, q, 97); q2[size] ^= 1;
    printf("%d\n", koji_ecdh_shared(which, k, q2, 1 + 2 * size, out));
    koji_ecdh_public(which, k, q);
    printf("%d", koji_ecdsa_verify(which, d, size, rb, sb, q, 1 + 2 * size));
    d[0] ^= 1;
    printf(" %d", koji_ecdsa_verify(which, d, size, rb, sb, q, 1 + 2 * size));
    d[0] ^= 1; memset(rb, 0, size);
    printf(" %d\n", koji_ecdsa_verify(which, d, size, rb, sb, q, 1 + 2 * size));
}
int main(int argc, char **argv) {
    koji_init(argc, argv);
    unsigned char p[24], b[24], e[8], out[256];
    load("ffffffffffffffffffffffffffffffffffffffffffffff13", p); load("31ff7a01ec99108ddb5b5fab8f4d3e27dda1494c73cf256d", b); load("73ab48767734d7c1", e);
    koji_modular_power(b, e, 8, p, 24, out); hex(out, 24);
    curve(0, 32, "0079cb9e830c71c2cdcc69292f45e678309d6b79965eda32dae445508201e2bd",
          "0411aedf597e4d8ef1ebafe7447e4b0e30c1c332c9d499039fb75c06a697a2703de278de90bc9517134fb56ab2e7bee141d5192b4e336ac211c9ca54150929b429",
          "323a4206f8d711e87a6ecd279280f40d91bd1a4ead247e21024aa59a93dd02c8", "f76e2504b59810cc960dba5b4a3c4caf951a134198cfab746edc42e9ed2befb8", "aa838ba211a332aa85594f75851024036563cddebab09004ab4295a9c24c5ccf");
    curve(1, 48, "0073f778f6fa5db8656abd72fb710734986e86cb0ab8ab67a26b7f62b1852f27e3eff9c0cf44dd3f89e7d15f17362f25",
          "049d09efbd81dce9bd44b3a3d211ea57df4c98d2727d7c45c07dfeb44ef8d794b01d6488fef6015c105391f32fbf55ea9ee0dba838cc3d85e41520efae1963986543339fb4146d3bf874db8b3d75c3a5f728638a8d919f0b55c6a1608b4ce04373",
          "727edb0adb4a6c144d725b954cacd5f1a55c3dfdb0fdefdf3fd62cd1d64c01bf5dc66eef1c0a004126031a406533de5a",
          "3852234f196e98a684befbb76b67714cc6dd3a7090feeb0d9272dda38fc68bdb722b13542bb6713f7d355a7cd134fd39",
          "fade094323216da50551091ff230828de95c6f4cdea10b52d7374d4049137ba9a1b1cd06163e2ceb335bba551cc0c313");
    unsigned char n[256], d[32], pkcs[256], pss[256], f[3] = {1, 0, 1};
    load("b0cd6cafd9aed48ea1c48b88edbd4183196c545566f5739f8c288bf39946155b9455d71a953db5673ec45e59e0292cb3039e6edafd604a6d2eb04826c45654aba7574b926c4abdc729bfb4880e8e18300ce48db4a7358d575ab49c95fdfbecdbdfbdae331ee6b8ec5a25c7659a0d557dc69266f628f5011159585f515e9c4339"
         "3b62b829f4e8102cc7c56825167ddb922435f1602319e74faf85adbbf4a7276e0ac717fdbd753cb7dfcb31ab8403100ef7c6eaf5393cefeebf2dfcb8008b4b29e9acf94e1a6ee48f7db02b1e8f6f3880249b4648d1a8a54e8ef37e2382fc57a2ab161ab137d25dbf1271c81caa4f3ebad889f593342ed229eb505a1ab718b00b", n);
    load("323a4206f8d711e87a6ecd279280f40d91bd1a4ead247e21024aa59a93dd02c8", d);
    load("0d19dc8b7e67e7c9acb9d1718aa2dc9f95755c143fc3e569e2a7d47dc6a94be1d669204c18165e41bc07d48590e1494d6c10b9b58109f63bbc9edb149f297b499ee60fe721ac800675eaf2fe0fbc02fc57d1aa16eaaeee914a8137b3549bc9795d20d07b8f7afd5d56347ebc8f899b3073d73843101aceded45ee01a760632bd"
         "32378b86d9c4ee9caa6b8c833f27fee2c31b2b6752a1e27164c5bb22020cfa22bff5ac18b7f2812dae963da279a36ff6d0e51d339205d12cc9af99b93f10291c389d2c52dbceed8602dd3a247ffa356947422d0d1be2e855e5c4bb3ac061ccd1410fa5edcf1ecce88ee91042617f84c0c7012e33e9e2ac10727a8f07eff125c6", pkcs);
    load("88261cd161d9c7fd831f59c97e76d3f7539c0f58ab7566b1a18a0f8081ea2a8493cf458976b04716c7e0010449be073c3572a5b976a93628dc41041269b9436712cd78a803be3f51c2fde69acd443aff92810f59a2d2025aa1e8d327c88f8e5d58aa1353511d804d3b2e5c3a00fc9d3d1ec9b18b9582727266f7d5fa4783df22"
         "c46e55a9b2033247906b9a8311db15b65613e7a69fe8bc0419775a7b94f035193c8d66eebc313444ecd44838f68a3aff8098435717abca5392dcdf3ee06577cd2d432499600db157f8e3f891eb6d6aaf9ac89d56683343a9ac73ed9490d01780f0d661c240354a8f0e7c89929f2220da2ecd97414dd54dcdf65bd65df268dfa5", pss);
    printf("%d %d", koji_rsa_verify(0, 0, d, 32, pkcs, 256, n, 256, f, 3), koji_rsa_verify(1, 0, d, 32, pss, 256, n, 256, f, 3));
    printf(" %d %d", koji_rsa_verify(1, 0, d, 32, pkcs, 256, n, 256, f, 3), koji_rsa_verify(0, 0, d, 32, pss, 256, n, 256, f, 3));
    d[31] ^= 1;
    printf(" %d %d", koji_rsa_verify(0, 0, d, 32, pkcs, 256, n, 256, f, 3), koji_rsa_verify(1, 0, d, 32, pss, 256, n, 256, f, 3));
    printf(" %d\n", koji_rsa_verify(0, 1, d, 32, pkcs, 256, n, 256, f, 3));
    return 0;
}
"#
    );
    assert_eq!(
        run_c("bignum", &source),
        "7ab0369e2706d0742f777e8e8f9934727a152bbdfb22f3b3\n\
         048c9b0a4e96afb44d402194d5b279eff71eb3a67e7f1442428edec32063059b77afbbe567aa896770059dff172f0537e73c09cc257d902360474a553045dde879\n\
         1 fb13c0e558bf09702a6387874cfd957a536336d7137728aadd47bd6aec0375ee\n0\n1 0 0\n\
         042975eaf65bfb82cebe69bb231b2c2dc900dcb2de3f36184df4a9b6cb670956a6cb03d23f9d0071893f9d88eb7d518dc55e9828e747526ca6c8c6832578c67afa981a8d1e20b12057d051e036cfab11bb7db7655345550818b642ce2603b266d4\n\
         1 e5b7eaa07cc57d33f3c635a8a581b33f8397fc55b6edc10d8028e3fb11b57837dc036f20ab250205cdf88a654eea3880\n0\n1 0 0\n\
         1 1 0 0 0 0 0\n"
    );
}
