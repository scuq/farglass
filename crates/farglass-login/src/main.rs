//! farglass-login: the PAM login and the session worker.

fn main() {
    println!(
        "{} {} (protocol {})",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        farglass_proto::PROTOCOL_VERSION
    );
}
