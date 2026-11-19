//! gen-ts writes the generated TypeScript files of the protocol to `web/src/proto/generated/`.
//!
//! Usage:
//!
//!     cargo run -p farglass-proto --example gen-ts

use std::error::Error;
use std::fs;
use std::path::Path;

use farglass_proto::typescript;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(typescript::OUTPUT_DIR);
    fs::create_dir_all(&dir)?;
    for (name, contents) in typescript::files() {
        let path = dir.join(name);
        fs::write(&path, contents)?;
        println!("wrote {}/{name}", typescript::OUTPUT_DIR);
    }
    Ok(())
}
