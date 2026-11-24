//! The use of the host by the apps of each window, and by farglass (F045, F046).
//!
//! An app that the launcher starts runs in a systemd --user scope
//! `app-farglass-*.scope`, and a terminal in `farglass-terminal-*.scope`. The
//! agent reads the CPU and the memory of that cgroup. For a process outside such
//! a scope, it reads the process itself. The network is the TCP traffic of the
//! processes of the scope, from `ss`: a user scope has no IP accounting.
//!
//! The agent also reads the use of the whole host from `/proc` (F105).
//!
//! The agent samples every [`INTERVAL`], and only while a stats channel is open.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use farglass_proto::message::{HostUsage, StatsEvent, Usage, WindowUsage};

use crate::surfaces::Surfaces;

/// The time between two samples.
pub const INTERVAL: Duration = Duration::from_secs(2);

/// What the agent measures: a cgroup, or one process.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// The directory of a cgroup in `/sys/fs/cgroup`.
    Cgroup(PathBuf),
    /// One process.
    Process(u32),
}

/// The counters of a source at one time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    /// The CPU time in microseconds.
    pub cpu_micros: u64,
    /// The memory in bytes.
    pub memory: u64,
    /// The TCP bytes received and sent by the processes, if known.
    pub net_bytes: Option<u64>,
}

/// Returns the cgroup of a process if it is a scope of one app or one terminal, else None.
#[must_use]
pub fn app_cgroup(pid: u32, root: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    let path = text.lines().find_map(|line| line.strip_prefix("0::"))?;
    let name = Path::new(path).file_name()?.to_str()?;
    let own = name.ends_with(".scope")
        && (name.starts_with("app-") || name.starts_with("farglass-terminal-"));
    own.then(|| root.join(path.trim_start_matches('/')))
}

/// Returns the source of a process: its scope, or the process itself.
#[must_use]
pub fn source_of(pid: u32) -> Source {
    app_cgroup(pid, Path::new("/sys/fs/cgroup")).map_or(Source::Process(pid), Source::Cgroup)
}

/// Reads `usage_usec` of a `cpu.stat` text.
fn usage_usec(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("usage_usec "))
        .and_then(|value| value.trim().parse().ok())
}

/// The clock ticks for each second of `/proc/PID/stat`.
const TICKS: u64 = 100;

/// Returns the CPU time in microseconds and the resident memory in bytes of a process.
fn process_counters(pid: u32) -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The name of the program can have spaces: the fields start after the last parenthesis.
    let fields: Vec<&str> = stat.get(stat.rfind(')')? + 2..)?.split(' ').collect();
    let ticks = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let pages: u64 = statm.split(' ').nth(1)?.parse().ok()?;
    Some((ticks * 1_000_000 / TICKS, pages * 4096))
}

/// Returns a process and each process under it.
fn tree(pid: u32) -> Vec<u32> {
    let mut all = vec![pid];
    let mut i = 0;
    while let Some(&next) = all.get(i) {
        i += 1;
        let children = std::fs::read_to_string(format!("/proc/{next}/task/{next}/children"))
            .unwrap_or_default();
        all.extend(
            children
                .split_whitespace()
                .filter_map(|child| child.parse::<u32>().ok()),
        );
        if all.len() > 4096 {
            break;
        }
    }
    all
}

/// Returns the processes of a source: a process with each process under it, or the processes of a cgroup.
fn pids_of(source: &Source) -> Vec<u32> {
    match source {
        Source::Process(pid) => tree(*pid),
        Source::Cgroup(dir) => std::fs::read_to_string(dir.join("cgroup.procs"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect(),
    }
}

/// Reads the counters of a source. `net` gives the TCP bytes of each process.
fn counters(source: &Source, net: Option<&HashMap<u32, u64>>) -> Option<Counters> {
    let (cpu_micros, memory) = match source {
        Source::Process(pid) => {
            // The process must run; the processes under it can end at any time.
            let first = process_counters(*pid)?;
            tree(*pid)[1..]
                .iter()
                .filter_map(|child| process_counters(*child))
                .fold(first, |(cpu, memory), (c, m)| (cpu + c, memory + m))
        }
        Source::Cgroup(dir) => (
            usage_usec(&std::fs::read_to_string(dir.join("cpu.stat")).ok()?)?,
            std::fs::read_to_string(dir.join("memory.current"))
                .ok()?
                .trim()
                .parse()
                .ok()?,
        ),
    };
    let net_bytes = net.map(|bytes| {
        pids_of(source)
            .iter()
            .filter_map(|pid| bytes.get(pid))
            .sum()
    });
    Some(Counters {
        cpu_micros,
        memory,
        net_bytes,
    })
}

/// Parses the output of `ss -H -t -i -n -p`: the bytes received and sent by the TCP sockets of each process.
#[must_use]
pub fn parse_ss(text: &str) -> HashMap<u32, u64> {
    let mut bytes: HashMap<u32, u64> = HashMap::new();
    let mut owners: Vec<u32> = Vec::new();
    for line in text.lines() {
        if line.contains("users:((") {
            owners = line
                .split("pid=")
                .skip(1)
                .filter_map(|rest| {
                    rest.split(|c: char| !c.is_ascii_digit())
                        .next()?
                        .parse()
                        .ok()
                })
                .collect();
            owners.dedup();
            continue;
        }
        let field = |name: &str| {
            line.split_whitespace()
                .find_map(|word| word.strip_prefix(name))
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0)
        };
        let total = field("bytes_received:") + field("bytes_sent:");
        if total > 0 {
            // A socket that several processes share counts for the first one.
            if let Some(owner) = owners.first() {
                *bytes.entry(*owner).or_default() += total;
            }
        }
        owners.clear();
    }
    bytes
}

/// Returns the TCP bytes of each process of the user, or None if `ss` does not run.
fn tcp_bytes() -> Option<HashMap<u32, u64>> {
    let output = std::process::Command::new("ss")
        .args(["-H", "-t", "-i", "-n", "-p"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_ss(&String::from_utf8_lossy(&output.stdout)))
}

/// Returns the use from two counters `elapsed` apart.
#[must_use]
pub fn usage(before: &Counters, now: &Counters, elapsed: Duration) -> Usage {
    let seconds = elapsed.as_secs_f64().max(0.001);
    let cpu = now.cpu_micros.saturating_sub(before.cpu_micros) as f64 / 1e6 / seconds * 100.0;
    // A closed socket takes its bytes out of the sum: the rate is not negative.
    let net = match (before.net_bytes, now.net_bytes) {
        (Some(a), Some(b)) => Some(b.saturating_sub(a) as f64 / seconds),
        _ => None,
    };
    Usage {
        cpu: (cpu * 10.0).round() / 10.0,
        memory: now.memory,
        net,
    }
}

/// The counters of the whole host at one time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostCounters {
    /// The clock ticks in which a CPU was not idle.
    pub busy: u64,
    /// The clock ticks of all CPUs.
    pub total: u64,
    /// The memory in use in bytes.
    pub memory: u64,
    /// The memory of the host in bytes.
    pub memory_total: u64,
    /// The bytes that the network devices received.
    pub received: u64,
    /// The bytes that the network devices sent.
    pub sent: u64,
}

/// Parses the first line of `/proc/stat`: the busy ticks and all ticks of the CPUs.
///
/// Idle and iowait are not busy. The guest times are part of user and nice.
#[must_use]
pub fn parse_proc_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let ticks: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map_while(|value| value.parse().ok())
        .collect();
    if ticks.len() < 4 {
        return None;
    }
    let total: u64 = ticks.iter().sum();
    let idle = ticks[3] + ticks.get(4).copied().unwrap_or(0);
    Some((total.saturating_sub(idle), total))
}

/// Parses `/proc/meminfo`: the memory in use and the memory of the host, in bytes.
#[must_use]
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .map(|kilobytes| kilobytes * 1024)
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    Some((total.saturating_sub(available), total))
}

/// Parses `/proc/net/dev`: the bytes received and sent by the interfaces that `keep` accepts.
#[must_use]
pub fn parse_net_dev(text: &str, keep: impl Fn(&str) -> bool) -> (u64, u64) {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            if !keep(name.trim()) {
                return None;
            }
            let fields: Vec<&str> = rest.split_whitespace().collect();
            Some((
                fields.first()?.parse::<u64>().ok()?,
                fields.get(8)?.parse::<u64>().ok()?,
            ))
        })
        .fold((0, 0), |(received, sent), (r, s)| (received + r, sent + s))
}

/// Reads the counters of the host, or None if `/proc` does not have them.
fn host_counters() -> Option<HostCounters> {
    let (busy, total) = parse_proc_stat(&std::fs::read_to_string("/proc/stat").ok()?)?;
    let (memory, memory_total) = parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)?;
    let net = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
    // A bridge and a virtual device carry the bytes of a hardware device again:
    // only the devices with hardware count. A host with none counts all but the loopback.
    let hardware = |name: &str| {
        Path::new("/sys/class/net")
            .join(name)
            .join("device")
            .exists()
    };
    let mut traffic = parse_net_dev(&net, hardware);
    if traffic == (0, 0) {
        traffic = parse_net_dev(&net, |name| name != "lo");
    }
    Some(HostCounters {
        busy,
        total,
        memory,
        memory_total,
        received: traffic.0,
        sent: traffic.1,
    })
}

/// Returns the use of the host from two counters `elapsed` apart.
#[must_use]
pub fn host_usage(before: &HostCounters, now: &HostCounters, elapsed: Duration) -> HostUsage {
    let seconds = elapsed.as_secs_f64().max(0.001);
    let total = now.total.saturating_sub(before.total);
    let cpu = if total == 0 {
        0.0
    } else {
        now.busy.saturating_sub(before.busy) as f64 / total as f64 * 100.0
    };
    // A device that goes away takes its bytes out of the sum: the rate is not negative.
    let rate = |before: u64, now: u64| (now.saturating_sub(before) as f64 / seconds).round();
    HostUsage {
        cpu: (cpu.min(100.0) * 10.0).round() / 10.0,
        memory: now.memory,
        memory_total: now.memory_total,
        net_in: rate(before.received, now.received),
        net_out: rate(before.sent, now.sent),
    }
}

type Sink = Arc<dyn Fn(&StatsEvent) + Send + Sync>;

/// A read of a source: the source, the time, and the counters.
type Read = (Source, Instant, Counters);

/// The stats channels and the sampler.
pub struct Stats {
    viewers: Mutex<HashMap<u64, Sink>>,
    next: AtomicU64,
    running: Mutex<bool>,
    surfaces: Arc<Surfaces>,
    terminals: Arc<Mutex<HashMap<String, u32>>>,
    server: Mutex<Option<u32>>,
}

impl std::fmt::Debug for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stats").finish_non_exhaustive()
    }
}

/// One stats channel. Dropping it removes the channel. With no channel, the sampler stops.
pub struct StatsGuard {
    stats: Arc<Stats>,
    id: u64,
}

impl std::fmt::Debug for StatsGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatsGuard").field("id", &self.id).finish()
    }
}

impl Drop for StatsGuard {
    fn drop(&mut self) {
        self.stats
            .viewers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

impl Stats {
    /// Returns the stats of the windows of `surfaces` and of the terminals in `terminals`.
    #[must_use]
    pub fn new(surfaces: Arc<Surfaces>, terminals: Arc<Mutex<HashMap<String, u32>>>) -> Arc<Self> {
        Arc::new(Self {
            viewers: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            running: Mutex::new(false),
            surfaces,
            terminals,
            server: Mutex::new(None),
        })
    }

    /// Sets the process of farglassd, from the credentials of the agent socket.
    pub fn set_server(&self, pid: Option<u32>) {
        *self.server.lock().unwrap_or_else(PoisonError::into_inner) = pid;
    }

    /// Adds a stats channel, and starts the sampler if it does not run.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(&StatsEvent) + Send + Sync + 'static,
    ) -> StatsGuard {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.viewers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::new(send));
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if !*running {
            *running = true;
            let stats = self.clone();
            tokio::spawn(async move { stats.sample_loop().await });
        }
        StatsGuard {
            stats: self.clone(),
            id,
        }
    }

    /// The sources of the windows: `surface-ID` for a Linux app, the terminal id for a terminal.
    fn window_sources(&self) -> Vec<(String, Source)> {
        let surfaces = self
            .surfaces
            .processes()
            .into_iter()
            .map(|(id, pid)| (format!("surface-{id}"), source_of(pid)));
        let terminals: Vec<(String, Source)> = self
            .terminals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(id, pid)| (id.clone(), source_of(*pid)))
            .collect();
        surfaces.chain(terminals).collect()
    }

    async fn sample_loop(self: Arc<Self>) {
        let mut last: HashMap<Source, (Instant, Counters)> = HashMap::new();
        let mut last_host: Option<(Instant, HostCounters)> = None;
        let mut timer = tokio::time::interval(INTERVAL);
        loop {
            timer.tick().await;
            let sinks: Vec<Sink> = self
                .viewers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .cloned()
                .collect();
            if sinks.is_empty() {
                *self.running.lock().unwrap_or_else(PoisonError::into_inner) = false;
                return;
            }
            let windows = self.window_sources();
            let server = *self.server.lock().unwrap_or_else(PoisonError::into_inner);
            let me = std::process::id();
            let read = tokio::task::spawn_blocking(move || {
                let net = tcp_bytes();
                let now = Instant::now();
                let read = |source: &Source| {
                    counters(source, net.as_ref()).map(|c| (source.clone(), now, c))
                };
                let windows: Vec<(String, Option<Read>)> = windows
                    .into_iter()
                    .map(|(window, source)| (window, read(&source)))
                    .collect();
                let agent = read(&Source::Process(me));
                let server = server.and_then(|pid| read(&Source::Process(pid)));
                let host = host_counters().map(|counters| (now, counters));
                (windows, agent, server, host)
            })
            .await;
            let Ok((windows, agent, server, host_now)) = read else {
                continue;
            };
            let mut next: HashMap<Source, (Instant, Counters)> = HashMap::new();
            let mut take = |sample: Option<Read>| -> Option<Usage> {
                let (source, at, now) = sample?;
                let usage = last
                    .get(&source)
                    .map(|(before_at, before)| usage(before, &now, at.duration_since(*before_at)));
                next.insert(source, (at, now));
                usage
            };
            let windows: Vec<WindowUsage> = windows
                .into_iter()
                .filter_map(|(window, sample)| {
                    take(sample).map(|usage| WindowUsage { window, usage })
                })
                .collect();
            let agent = take(agent);
            let server = take(server);
            last = next;
            let host = match (&last_host, &host_now) {
                (Some((before_at, before)), Some((at, now))) => {
                    Some(host_usage(before, now, at.duration_since(*before_at)))
                }
                _ => None,
            };
            last_host = host_now;
            // The first sample has no interval yet.
            let Some(agent) = agent else {
                continue;
            };
            let event = StatsEvent::Sample {
                windows,
                agent,
                server,
                host,
            };
            for send in &sinks {
                send(&event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tcp_bytes_of_ss_go_to_the_first_process_of_each_socket() {
        let text = "ESTAB 0 0 10.0.0.2:5000 1.2.3.4:443 users:((\"brave\",pid=4242,fd=30),(\"brave\",pid=4243,fd=12))\n\t cubic wscale:7,7 rto:204 bytes_sent:1500 bytes_acked:1500 bytes_received:98000 segs_out:20\nESTAB 0 0 10.0.0.2:5001 1.2.3.4:443 users:((\"brave\",pid=4242,fd=31))\n\t cubic bytes_received:2000\nESTAB 0 0 10.0.0.2:5002 1.2.3.4:443\n\t cubic bytes_received:7\n";
        let bytes = parse_ss(text);
        assert_eq!(bytes.get(&4242), Some(&(1500 + 98000 + 2000)));
        assert_eq!(bytes.get(&4243), None);
        assert_eq!(
            bytes.len(),
            1,
            "a socket of a different user has no process"
        );
    }

    #[test]
    fn the_use_is_the_difference_over_the_interval() {
        let before = Counters {
            cpu_micros: 1_000_000,
            memory: 10,
            net_bytes: Some(5_000),
        };
        let now = Counters {
            cpu_micros: 1_500_000,
            memory: 20,
            net_bytes: Some(4_000),
        };
        let usage = usage(&before, &now, Duration::from_secs(2));
        assert!((usage.cpu - 25.0).abs() < 0.01);
        assert_eq!(usage.memory, 20);
        assert_eq!(
            usage.net,
            Some(0.0),
            "a closed socket gives no negative rate"
        );
    }

    #[test]
    fn the_host_counters_parse_and_give_the_use_over_the_interval() {
        let stat = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\n";
        assert_eq!(parse_proc_stat(stat), Some((150, 1000)));
        assert_eq!(parse_proc_stat("intr 5\n"), None);
        let meminfo = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:   12000000 kB\n";
        assert_eq!(
            parse_meminfo(meminfo),
            Some((4_000_000 * 1024, 16_000_000 * 1024))
        );
        let dev = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 500 5 0 0 0 0 0 0 500 5 0 0 0 0 0 0\n  eth0: 9000 9 0 0 0 0 0 0 3000 3 0 0 0 0 0 0\ndocker0: 70 1 0 0 0 0 0 0 80 1 0 0 0 0 0 0\n";
        assert_eq!(parse_net_dev(dev, |name| name == "eth0"), (9000, 3000));
        assert_eq!(parse_net_dev(dev, |name| name != "lo"), (9070, 3080));

        let before = HostCounters {
            busy: 100,
            total: 1000,
            memory: 1,
            memory_total: 8,
            received: 10_000,
            sent: 5_000,
        };
        let now = HostCounters {
            busy: 150,
            total: 1200,
            memory: 2,
            memory_total: 8,
            received: 14_000,
            sent: 4_000,
        };
        let usage = host_usage(&before, &now, Duration::from_secs(2));
        assert!((usage.cpu - 25.0).abs() < 0.01);
        assert_eq!((usage.memory, usage.memory_total), (2, 8));
        assert!((usage.net_in - 2000.0).abs() < 0.01);
        assert!(
            usage.net_out.abs() < 0.01,
            "a device that went away gives no negative rate"
        );
        assert!(host_counters().is_some_and(|host| host.memory_total > 0 && host.total > 0));
    }

    #[test]
    fn the_counters_of_this_process_and_the_cpu_stat_parse() {
        let (cpu, memory) = process_counters(std::process::id()).unwrap();
        // A new test process can have used less than one clock tick of CPU.
        let _ = cpu;
        assert!(memory > 0);
        assert_eq!(usage_usec("usage_usec 1234\nuser_usec 1000\n"), Some(1234));
    }
}
