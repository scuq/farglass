//! Makes sure that the generated TypeScript files in the repository are current.

use std::fs;
use std::path::Path;

use farglass_proto::typescript;

#[test]
fn generated_typescript_is_current() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(typescript::OUTPUT_DIR);
    for (name, expected) in typescript::files() {
        let path = dir.join(name);
        let actual = fs::read_to_string(&path).unwrap_or_default();
        assert!(
            actual == expected,
            "{}/{name} is old. Run: cargo run -p farglass-proto --example gen-ts",
            typescript::OUTPUT_DIR
        );
    }
}
