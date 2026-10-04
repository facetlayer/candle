//! Telling a recorded process from an unrelated one that was later given the
//! same PID.
//!
//! A `processes` row outlives its process when the monitor is killed or the
//! machine reboots, and the OS reuses PIDs. A bare "is this PID alive" check
//! would then report the service as running and let `kill` signal a stranger.
//! So each row also records the OS start time of its PIDs, and a PID only
//! counts as the recorded process while its start time still matches.

use crate::process_alive::is_process_alive;

/// The OS start time of `pid` as an opaque number: equal for the same process,
/// different for a later process with the same PID. `None` if the process
/// doesn't exist, can't be inspected (it belongs to another user), or the
/// platform has no way to ask.
pub fn process_start_token(pid: i64) -> Option<i64> {
    if pid <= 0 {
        return None;
    }
    start_token(pid)
}

#[cfg(target_os = "macos")]
fn start_token(pid: i64) -> Option<i64> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let written = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut libc::proc_bsdinfo as *mut libc::c_void,
            size,
        )
    };
    (written == size)
        .then(|| info.pbi_start_tvsec as i64 * 1_000_000 + info.pbi_start_tvusec as i64)
}

/// Field 22 of `/proc/<pid>/stat`: the start time in clock ticks since boot.
#[cfg(target_os = "linux")]
fn start_token(pid: i64) -> Option<i64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_proc_stat_start_time(&stat)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn start_token(_pid: i64) -> Option<i64> {
    None
}

/// The command name (field 2) is in parentheses and may itself contain spaces
/// and parentheses, so count fields from the last `)`: the next one is field 3.
#[cfg(any(target_os = "linux", test))]
fn parse_proc_stat_start_time(stat: &str) -> Option<i64> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    after_name.split_whitespace().nth(22 - 3)?.parse().ok()
}

/// Whether `pid` is alive and is still the process a row recorded.
///
/// `recorded` is the start token stored when the row was written. Rows written
/// by an older candle have none, and for those a live PID is all there is to
/// go on.
pub fn is_recorded_process(pid: i64, recorded: Option<i64>) -> bool {
    if !is_process_alive(pid) {
        return false;
    }
    match recorded {
        None => true,
        Some(recorded) => {
            if cfg!(any(target_os = "macos", target_os = "linux")) {
                process_start_token(pid) == Some(recorded)
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_token_is_stable_and_matches() {
        let me = std::process::id() as i64;
        let token = process_start_token(me);
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert!(token.is_some());
        }
        assert_eq!(token, process_start_token(me));
        assert!(is_recorded_process(me, token));
        // No recorded token: a live PID is enough.
        assert!(is_recorded_process(me, None));
    }

    #[test]
    fn a_different_start_time_is_a_different_process() {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return;
        }
        let me = std::process::id() as i64;
        let token = process_start_token(me).unwrap();
        assert!(!is_recorded_process(me, Some(token + 1)));
    }

    #[test]
    fn a_child_has_its_own_token() {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return;
        }
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id() as i64;
        let token = process_start_token(pid);
        assert!(token.is_some());
        assert!(is_recorded_process(pid, token));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!is_recorded_process(pid, token));
    }

    #[test]
    fn dead_or_invalid_pids_have_no_token() {
        assert_eq!(process_start_token(0), None);
        assert_eq!(process_start_token(-1), None);
        assert_eq!(process_start_token(2_000_000_000), None);
        assert!(!is_recorded_process(2_000_000_000, None));
    }

    #[test]
    fn parses_start_time_past_an_awkward_command_name() {
        let stat = "1234 (my (odd) name) S 1 1234 1234 0 -1 4194304 100 0 0 0 5 3 0 0 20 0 1 0 \
                    987654 1000000 200 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0";
        assert_eq!(parse_proc_stat_start_time(stat), Some(987654));
        assert_eq!(parse_proc_stat_start_time("garbage"), None);
    }
}
