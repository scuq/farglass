//! farglassd: the web server and the channel router on the host.

fn main() {
    println!(
        "{} {} (protocol {})",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        farglass_proto::PROTOCOL_VERSION
    );
}
