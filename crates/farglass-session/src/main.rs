//! farglass-session: the agent in the logind session of a user.

fn main() {
    println!(
        "{} {} (protocol {})",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        farglass_proto::PROTOCOL_VERSION
    );
}
