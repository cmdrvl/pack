//! The manifest bytes and `pack_id` written by the released pack 0.7.2 binary
//! (pack-0.7.2-x86_64-apple-darwin.tar.gz, sha256
//! 5d0926992fcf1ca73c93389b8dae19442896382e11129e7bcb47f1e9945847b9) must be reproduced exactly.
//! The fixture carries non-ASCII strings, including U+E000 and U+FB33 next to characters above
//! U+FFFF. Do not regenerate it from the code under test. The released witness lines are checked
//! by a unit test in `src/witness/record.rs`.

use pack::seal::manifest::Manifest;

const MANIFEST: &str = include_str!("../fixtures/released-0.7.2/manifest.json");

#[test]
fn released_manifest_bytes_and_pack_id_are_reproduced() {
    for astral_or_private_use in ['\u{e000}', '\u{fb33}', '\u{10000}', '\u{1f600}'] {
        assert!(MANIFEST.contains(astral_or_private_use));
    }
    let manifest: Manifest = serde_json::from_str(MANIFEST).unwrap();

    assert_eq!(manifest.to_canonical_bytes(), MANIFEST.as_bytes());
    assert_eq!(manifest.recompute_pack_id(), manifest.pack_id);

    let mut resealed = manifest.clone();
    resealed.finalize();
    assert_eq!(resealed.pack_id, manifest.pack_id);
}
