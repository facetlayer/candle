//! Listening-socket discovery, per platform.
//!
//! `list-ports` needs to know which TCP ports a set of PIDs are listening on.
//! Each platform has a different way to answer that, so this module hides the
//! differences behind [`listening_sockets_for_pids`]:
//!
//! - **Linux:** read `/proc/net/tcp` and `/proc/net/tcp6` for sockets in the
//!   `LISTEN` state, then map each socket inode back to a PID by reading the
//!   `/proc/<pid>/fd` links of the requested PIDs. No external tools needed, and
//!   it works without root for processes owned by the current user (which is
//!   all Candle ever manages). Falls back to `lsof` if `/proc/net/tcp` is
//!   unreadable.
//! - **macOS (and other Unixes):** `lsof -iTCP -sTCP:LISTEN -n -P`.
//! - **Windows:** `netstat -ano -p TCP`, keeping `LISTENING` rows.
//!
//! Every parser is plain string handling with no platform dependency, so all of
//! them are unit-tested on every host. Only the code that runs a tool or reads
//! `/proc` is `cfg`-gated.
//!
//! When the platform's method is unavailable (tool not installed, `/proc`
//! unreadable) the lookup fails with a [`PortLookupError`] that says what was
//! missing and how to fix it, so `list-ports` reports that instead of a
//! misleading "No open ports found".

use std::collections::HashSet;
use std::fmt;
use std::io;
use std::process::{Command, Stdio};

/// A listening socket, before it is attributed to a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListeningSocket {
    pub pid: i64,
    pub port: i64,
    /// Local address. Wildcard IPv4 is `0.0.0.0`; IPv6 addresses are in
    /// brackets, e.g. `[::1]` or `[::]`.
    pub address: String,
    /// Always `TCP` today; kept as a field so the JSON shape can grow.
    pub protocol: String,
}

/// Why listening ports could not be detected on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortLookupError {
    /// The command-line tool this platform relies on is not installed.
    ToolNotFound {
        tool: &'static str,
        hint: &'static str,
    },
    /// The tool exists but could not be run (permissions, spawn failure).
    ToolFailed { tool: &'static str, detail: String },
    /// Linux: `/proc/net/tcp` could not be read, and the `lsof` fallback was
    /// unavailable too.
    ProcUnavailable {
        detail: String,
        fallback: Box<PortLookupError>,
    },
}

impl fmt::Display for PortLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Could not detect listening ports: ")?;
        match self {
            PortLookupError::ToolNotFound { tool, hint } => {
                write!(f, "the `{tool}` command was not found. {hint}")
            }
            PortLookupError::ToolFailed { tool, detail } => {
                write!(f, "running `{tool}` failed ({detail}).")
            }
            PortLookupError::ProcUnavailable { detail, fallback } => {
                let fallback_text = fallback.to_string();
                let fallback_text = fallback_text
                    .strip_prefix("Could not detect listening ports: ")
                    .unwrap_or(&fallback_text);
                write!(
                    f,
                    "/proc/net/tcp is not readable ({detail}), and {fallback_text}"
                )
            }
        }
    }
}

impl std::error::Error for PortLookupError {}

const LSOF_HINT: &str = "Candle uses lsof to find which ports a service is listening on. \
    Install it with your package manager (e.g. `apt install lsof`, `dnf install lsof`, \
    `apk add lsof`) and try again.";

const NETSTAT_HINT: &str = "Candle uses netstat to find which ports a service is listening on. \
    It ships with Windows in System32; check that it is on your PATH.";

/// Find every listening TCP socket owned by one of `pids`.
///
/// Sockets are deduplicated by `pid:port`: a server that listens on both IPv4
/// and IPv6 (or that `lsof` reports twice) is shown once, with whichever
/// address came first.
///
/// An empty `pids` set returns `Ok(vec![])` without touching the system, so a
/// project with nothing running never fails for a missing tool.
pub fn listening_sockets_for_pids(
    pids: &HashSet<i64>,
) -> Result<Vec<ListeningSocket>, PortLookupError> {
    if pids.is_empty() {
        return Ok(Vec::new());
    }
    let sockets = platform::listening_sockets(pids)?;
    Ok(dedup_by_pid_port(
        sockets
            .into_iter()
            .filter(|s| pids.contains(&s.pid))
            .collect(),
    ))
}

fn dedup_by_pid_port(sockets: Vec<ListeningSocket>) -> Vec<ListeningSocket> {
    let mut seen: HashSet<(i64, i64)> = HashSet::new();
    sockets
        .into_iter()
        .filter(|s| seen.insert((s.pid, s.port)))
        .collect()
}

/// Run `command args` and return its stdout.
///
/// A missing executable becomes `ToolNotFound`; any other spawn failure becomes
/// `ToolFailed`. A non-zero exit is *not* an error: `lsof` exits 1 when nothing
/// matches, and `netstat` may warn on stderr while still printing the table.
fn run_capture_stdout(
    tool: &'static str,
    args: &[&str],
    hint: &'static str,
) -> Result<String, PortLookupError> {
    let output = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => PortLookupError::ToolNotFound { tool, hint },
            _ => PortLookupError::ToolFailed {
                tool,
                detail: e.to_string(),
            },
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Split `host:port` at the last colon (so bracketed IPv6 works), normalizing
/// the wildcard spellings `*` and `0.0.0.0` to `0.0.0.0`.
fn split_address_port(name: &str) -> Option<(String, i64)> {
    let idx = name.rfind(':')?;
    let (address, port) = (&name[..idx], &name[idx + 1..]);
    let port: i64 = port.parse().ok()?;
    let address = if address == "*" { "0.0.0.0" } else { address };
    Some((address.to_string(), port))
}

// ---------------------------------------------------------------------------
// Platform dispatch
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub fn listening_sockets(pids: &HashSet<i64>) -> Result<Vec<ListeningSocket>, PortLookupError> {
        match linux::listening_sockets_from_proc(pids) {
            Ok(sockets) => Ok(sockets),
            // /proc/net unavailable (unusual container setups) — try lsof, and
            // if that's missing too, report both problems.
            Err(detail) => {
                lsof::listening_sockets().map_err(|fallback| PortLookupError::ProcUnavailable {
                    detail,
                    fallback: Box::new(fallback),
                })
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    pub fn listening_sockets(
        _pids: &HashSet<i64>,
    ) -> Result<Vec<ListeningSocket>, PortLookupError> {
        windows::listening_sockets()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    use super::*;

    pub fn listening_sockets(
        _pids: &HashSet<i64>,
    ) -> Result<Vec<ListeningSocket>, PortLookupError> {
        lsof::listening_sockets()
    }
}

// ---------------------------------------------------------------------------
// lsof (macOS, other Unixes, Linux fallback)
// ---------------------------------------------------------------------------

#[cfg_attr(target_os = "windows", allow(dead_code))]
mod lsof {
    use super::*;

    pub fn listening_sockets() -> Result<Vec<ListeningSocket>, PortLookupError> {
        let stdout = run_capture_stdout("lsof", &["-iTCP", "-sTCP:LISTEN", "-n", "-P"], LSOF_HINT)?;
        Ok(parse(&stdout))
    }

    /// Parse `lsof -iTCP -sTCP:LISTEN -n -P` output.
    ///
    /// Only `LISTEN` lines with at least 9 whitespace-separated fields are
    /// considered. PID is field 1; the address is the second-to-last field (the
    /// last is `(LISTEN)`), split at its last `:`.
    pub fn parse(output: &str) -> Vec<ListeningSocket> {
        let mut sockets = Vec::new();
        for line in output.lines() {
            if !line.contains("LISTEN") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }
            let Ok(pid) = parts[1].parse::<i64>() else {
                continue;
            };
            let protocol = parts
                .iter()
                .find(|p| **p == "TCP" || **p == "UDP")
                .copied()
                .unwrap_or("TCP")
                .to_string();
            let Some((address, port)) = split_address_port(parts[parts.len() - 2]) else {
                continue;
            };
            sockets.push(ListeningSocket {
                pid,
                port,
                address,
                protocol,
            });
        }
        sockets
    }
}

// ---------------------------------------------------------------------------
// Linux: /proc/net/tcp{,6} + /proc/<pid>/fd
// ---------------------------------------------------------------------------

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod linux {
    use super::*;
    use std::collections::HashMap;
    use std::net::{Ipv4Addr, Ipv6Addr};

    /// Socket state `LISTEN` in `/proc/net/tcp`'s `st` column.
    const TCP_LISTEN: &str = "0A";

    /// Fails (with the I/O error text) only when `/proc/net/tcp` itself can't
    /// be read; the caller may then fall back to another method.
    pub fn listening_sockets_from_proc(
        pids: &HashSet<i64>,
    ) -> Result<Vec<ListeningSocket>, String> {
        let tcp4 = std::fs::read_to_string("/proc/net/tcp").map_err(|e| e.to_string())?;
        // IPv6 may be disabled on the host; treat that file as optional.
        let tcp6 = std::fs::read_to_string("/proc/net/tcp6").unwrap_or_default();

        // IPv4 first so a dual-stack listener shows its 0.0.0.0 address after
        // the pid:port dedup.
        let mut listeners = parse_proc_net_tcp(&tcp4);
        listeners.extend(parse_proc_net_tcp(&tcp6));
        if listeners.is_empty() {
            return Ok(Vec::new());
        }

        let inode_to_pid = socket_inodes_for_pids(pids);
        Ok(listeners
            .into_iter()
            .filter_map(|l| {
                let pid = *inode_to_pid.get(&l.inode)?;
                Some(ListeningSocket {
                    pid,
                    port: l.port,
                    address: l.address,
                    protocol: "TCP".to_string(),
                })
            })
            .collect())
    }

    /// A `LISTEN` row from `/proc/net/tcp` or `tcp6`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProcListener {
        pub address: String,
        pub port: i64,
        pub inode: u64,
    }

    /// Parse the kernel's `/proc/net/tcp` / `tcp6` table, keeping `LISTEN` rows.
    ///
    /// Columns: `sl local_address rem_address st tx_queue:rx_queue tr:tm->when
    /// retrnsmt uid timeout inode ...`. `local_address` is `HEXADDR:HEXPORT`
    /// with the address in the kernel's in-memory byte order (little-endian on
    /// every platform Candle ships for).
    pub fn parse_proc_net_tcp(contents: &str) -> Vec<ProcListener> {
        let mut out = Vec::new();
        for line in contents.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 10 || parts[3] != TCP_LISTEN {
                continue;
            }
            let Some((addr_hex, port_hex)) = parts[1].split_once(':') else {
                continue;
            };
            let Ok(port) = i64::from_str_radix(port_hex, 16) else {
                continue;
            };
            let Some(address) = decode_hex_address(addr_hex) else {
                continue;
            };
            let Ok(inode) = parts[9].parse::<u64>() else {
                continue;
            };
            out.push(ProcListener {
                address,
                port,
                inode,
            });
        }
        out
    }

    /// Decode the hex address column: 8 hex digits for IPv4 (one little-endian
    /// u32), 32 for IPv6 (four little-endian u32 words).
    pub fn decode_hex_address(hex: &str) -> Option<String> {
        match hex.len() {
            8 => {
                let word = u32::from_str_radix(hex, 16).ok()?;
                Some(Ipv4Addr::from(word.swap_bytes()).to_string())
            }
            32 => {
                let mut octets = [0u8; 16];
                for (i, chunk) in octets.chunks_mut(4).enumerate() {
                    let word = u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).ok()?;
                    chunk.copy_from_slice(&word.swap_bytes().to_be_bytes());
                }
                Some(format!("[{}]", Ipv6Addr::from(octets)))
            }
            _ => None,
        }
    }

    /// Map socket inode → PID for the given PIDs by reading their
    /// `/proc/<pid>/fd/*` links, which look like `socket:[12345]`.
    fn socket_inodes_for_pids(pids: &HashSet<i64>) -> HashMap<u64, i64> {
        let mut map = HashMap::new();
        for pid in pids {
            let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(target) = std::fs::read_link(entry.path()) else {
                    continue;
                };
                if let Some(inode) = parse_socket_link(&target.to_string_lossy()) {
                    map.insert(inode, *pid);
                }
            }
        }
        map
    }

    /// `socket:[12345]` → `12345`.
    pub fn parse_socket_link(target: &str) -> Option<u64> {
        target
            .strip_prefix("socket:[")?
            .strip_suffix(']')?
            .parse()
            .ok()
    }
}

// ---------------------------------------------------------------------------
// Windows: netstat
// ---------------------------------------------------------------------------

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
mod windows {
    use super::*;

    pub fn listening_sockets() -> Result<Vec<ListeningSocket>, PortLookupError> {
        let stdout = run_capture_stdout("netstat", &["-ano", "-p", "TCP"], NETSTAT_HINT)?;
        Ok(parse(&stdout))
    }

    /// Parse `netstat -ano -p TCP` output. Rows look like:
    ///
    /// ```text
    ///   TCP    0.0.0.0:3000    0.0.0.0:0    LISTENING    1234
    ///   TCP    [::]:3000       [::]:0       LISTENING    1234
    /// ```
    ///
    /// The state column is localized on non-English Windows. We accept any
    /// state token starting with `LISTEN` (English) or `ABH` (German
    /// `ABHÖREN`), the two spellings seen in the wild; the row shape
    /// `proto local foreign state pid` is the same everywhere.
    pub fn parse(output: &str) -> Vec<ListeningSocket> {
        let mut sockets = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() != 5 || !parts[0].eq_ignore_ascii_case("TCP") {
                continue;
            }
            let state = parts[3].to_ascii_uppercase();
            if !(state.starts_with("LISTEN") || state.starts_with("ABH")) {
                continue;
            }
            let Ok(pid) = parts[4].parse::<i64>() else {
                continue;
            };
            let Some((address, port)) = split_address_port(parts[1]) else {
                continue;
            };
            sockets.push(ListeningSocket {
                pid,
                port,
                address,
                protocol: "TCP".to_string(),
            });
        }
        sockets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sock(pid: i64, port: i64, address: &str) -> ListeningSocket {
        ListeningSocket {
            pid,
            port,
            address: address.to_string(),
            protocol: "TCP".to_string(),
        }
    }

    // --- lsof -------------------------------------------------------------

    #[test]
    fn lsof_parse_ipv4_and_star() {
        let out = "\
COMMAND   PID   USER   FD   TYPE DEVICE SIZE/OFF NODE NAME
node    12345   user   45u  IPv4 0x1234    0t0  TCP 127.0.0.1:3000 (LISTEN)
node    12345   user   46u  IPv4 0x1235    0t0  TCP *:8080 (LISTEN)
";
        let parsed = lsof::parse(out);
        assert_eq!(
            parsed,
            vec![sock(12345, 3000, "127.0.0.1"), sock(12345, 8080, "0.0.0.0")]
        );
    }

    #[test]
    fn lsof_parse_ipv6_splits_on_last_colon() {
        let out = "node    222   user   7u  IPv6 0xabc    0t0  TCP [::1]:5173 (LISTEN)\n";
        assert_eq!(lsof::parse(out), vec![sock(222, 5173, "[::1]")]);
    }

    #[test]
    fn lsof_non_listen_and_short_lines_skipped() {
        let out = "\
node    1   u   7u  IPv4 0x1 0t0 TCP 127.0.0.1:3000 (ESTABLISHED)
short line LISTEN
";
        assert!(lsof::parse(out).is_empty());
    }

    // --- Linux /proc --------------------------------------------------------

    #[test]
    fn proc_decode_ipv4_little_endian() {
        assert_eq!(linux::decode_hex_address("0100007F").unwrap(), "127.0.0.1");
        assert_eq!(linux::decode_hex_address("00000000").unwrap(), "0.0.0.0");
        assert_eq!(linux::decode_hex_address("0F02000A").unwrap(), "10.0.2.15");
    }

    #[test]
    fn proc_decode_ipv6_words() {
        assert_eq!(
            linux::decode_hex_address("00000000000000000000000000000000").unwrap(),
            "[::]"
        );
        assert_eq!(
            linux::decode_hex_address("00000000000000000000000001000000").unwrap(),
            "[::1]"
        );
        // ::ffff:127.0.0.1 (v4-mapped), as the kernel prints it.
        assert_eq!(
            linux::decode_hex_address("0000000000000000FFFF00000100007F").unwrap(),
            "[::ffff:127.0.0.1]"
        );
        assert!(linux::decode_hex_address("abc").is_none());
    }

    #[test]
    fn proc_net_tcp_keeps_listen_rows_only() {
        let contents = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41234 1 0000000000000000 100 0 0 10 0
   1: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41235 1 0000000000000000 100 0 0 10 0
   2: 0100007F:D431 0100007F:0BB8 01 00000000:00000000 00:00000000 00000000  1000        0 41236 1 0000000000000000 20 4 30 10 -1
";
        let parsed = linux::parse_proc_net_tcp(contents);
        assert_eq!(
            parsed,
            vec![
                linux::ProcListener {
                    address: "127.0.0.1".to_string(),
                    port: 3000,
                    inode: 41234
                },
                linux::ProcListener {
                    address: "0.0.0.0".to_string(),
                    port: 8080,
                    inode: 41235
                },
            ]
        );
    }

    #[test]
    fn proc_net_tcp6_row() {
        let contents = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1433 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 99 1 0000000000000000 100 0 0 10 0
";
        let parsed = linux::parse_proc_net_tcp(contents);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].address, "[::1]");
        assert_eq!(parsed[0].port, 5171);
        assert_eq!(parsed[0].inode, 99);
    }

    #[test]
    fn proc_socket_link() {
        assert_eq!(linux::parse_socket_link("socket:[41234]"), Some(41234));
        assert_eq!(linux::parse_socket_link("/dev/null"), None);
        assert_eq!(linux::parse_socket_link("pipe:[7]"), None);
    }

    // --- Windows netstat ----------------------------------------------------

    #[test]
    fn netstat_parse_listening_rows() {
        let out = "\r\n\
Active Connections\r\n\
\r\n\
  Proto  Local Address          Foreign Address        State           PID\r\n\
  TCP    0.0.0.0:3000           0.0.0.0:0              LISTENING       1234\r\n\
  TCP    127.0.0.1:5173         0.0.0.0:0              LISTENING       5678\r\n\
  TCP    127.0.0.1:52000        127.0.0.1:3000         ESTABLISHED     9999\r\n\
  TCP    [::]:3000              [::]:0                 LISTENING       1234\r\n\
  TCP    [::1]:5173             [::]:0                 LISTENING       5678\r\n\
  UDP    0.0.0.0:5353           *:*                                    4321\r\n";
        let parsed = windows::parse(out);
        assert_eq!(
            parsed,
            vec![
                sock(1234, 3000, "0.0.0.0"),
                sock(5678, 5173, "127.0.0.1"),
                sock(1234, 3000, "[::]"),
                sock(5678, 5173, "[::1]"),
            ]
        );
    }

    // --- errors -------------------------------------------------------------

    #[test]
    fn missing_tool_is_reported_with_hint() {
        let err = run_capture_stdout("candle-definitely-not-a-real-tool", &[], "Install it.")
            .unwrap_err();
        assert_eq!(
            err,
            PortLookupError::ToolNotFound {
                tool: "candle-definitely-not-a-real-tool",
                hint: "Install it."
            }
        );
        assert_eq!(
            err.to_string(),
            "Could not detect listening ports: the `candle-definitely-not-a-real-tool` command was not found. Install it."
        );
    }

    #[test]
    fn proc_unavailable_message_mentions_both_causes() {
        let err = PortLookupError::ProcUnavailable {
            detail: "Permission denied (os error 13)".to_string(),
            fallback: Box::new(PortLookupError::ToolNotFound {
                tool: "lsof",
                hint: "Install lsof.",
            }),
        };
        assert_eq!(
            err.to_string(),
            "Could not detect listening ports: /proc/net/tcp is not readable (Permission denied (os error 13)), \
             and the `lsof` command was not found. Install lsof."
        );
    }

    // --- shared -------------------------------------------------------------

    #[test]
    fn dedup_keeps_first_address_per_pid_port() {
        let deduped = dedup_by_pid_port(vec![
            sock(1, 3000, "0.0.0.0"),
            sock(1, 3000, "[::]"),
            sock(2, 3000, "[::]"),
        ]);
        assert_eq!(
            deduped,
            vec![sock(1, 3000, "0.0.0.0"), sock(2, 3000, "[::]")]
        );
    }

    #[test]
    fn empty_pid_set_is_ok_and_empty() {
        assert_eq!(listening_sockets_for_pids(&HashSet::new()), Ok(Vec::new()));
    }

    #[test]
    fn finds_own_listening_socket() {
        // End-to-end on the host: bind a port, then ask for our own PID.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port() as i64;
        let me = std::process::id() as i64;
        let pids: HashSet<i64> = [me].into_iter().collect();
        let found = listening_sockets_for_pids(&pids).expect("port lookup should work on the host");
        assert!(
            found.iter().any(|s| s.pid == me && s.port == port),
            "expected port {port} for pid {me}, got {found:?}"
        );
        assert!(found.iter().all(|s| s.pid == me));
    }
}
