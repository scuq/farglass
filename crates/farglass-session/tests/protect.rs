//! The agent closes itself to a debugger of the user (F057).

use std::time::Duration;

#[test]
fn a_process_of_the_user_cannot_read_the_memory_or_the_environment_of_the_agent() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = std::process::Command::new(env!("CARGO_BIN_EXE_farglass-session"))
        .env("FARGLASS_AGENT_SOCKET", dir.path().join("none.sock"))
        .env("XDG_RUNTIME_DIR", dir.path())
        .env_remove("WAYLAND_DISPLAY")
        .env("FARGLASS_REDIRECT_SOCKET", dir.path().join("redirect.sock"))
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let pid = agent.id();
    let environ = std::fs::read(format!("/proc/{pid}/environ"));
    let mem = std::fs::File::open(format!("/proc/{pid}/mem"));
    let _ = agent.kill();
    let _ = agent.wait();
    assert_eq!(
        environ.map_err(|error| error.kind()),
        Err(std::io::ErrorKind::PermissionDenied),
        "the environment of the agent is open to the user"
    );
    assert!(mem.is_err(), "the memory of the agent is open to the user");
}
