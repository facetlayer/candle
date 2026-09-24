// Hand-rolled argument parser for the candle CLI.
//
// We deliberately do not use clap: the Vitest suite asserts on specific CLI behavior — the literal
// substring `Unknown argument` for unrecognized flags, `Unrecognized command '<cmd>'`, exact grouped
// help, and exit-code conventions. A small hand-rolled parser reproduces these precisely.

use std::collections::{HashMap, HashSet};

/// Resolve a command token (including aliases) to its canonical name, or None if unrecognized.
pub fn canonical_command(token: &str) -> Option<&'static str> {
    let canonical = match token {
        "start" | "run" => "start",
        "check-start" => "check-start",
        "restart" => "restart",
        "kill" | "stop" => "kill",
        "kill-all" => "kill-all",
        "find-orphans" => "find-orphans",
        "list" | "ls" => "list",
        "ps" | "status" => "ps",
        "list-all" => "list-all",
        "logs" => "logs",
        "watch" => "watch",
        "wait-for-log" => "wait-for-log",
        "list-ports" => "list-ports",
        "list-ports-all" => "list-ports-all",
        "open-browser" => "open-browser",
        "setup-project" => "setup-project",
        "add-service" => "add-service",
        "remove-service" => "remove-service",
        "set-config" => "set-config",
        "clear-logs" => "clear-logs",
        "erase-database" => "erase-database",
        "list-docs" => "list-docs",
        "get-doc" => "get-doc",
        "help" => "help",
        "mcp" => "mcp",
        _ => return None,
    };
    Some(canonical)
}

/// The option spec for a canonical command: (flag-name, takes-a-value).
fn option_spec(command: &str) -> &'static [(&'static str, bool)] {
    match command {
        "start" => &[
            ("shell", true),
            ("root", true),
            ("enable-stdin", false),
            ("bg", false),
            ("watch", false),
            ("exit-after-ms", true),
            ("project-dir", true),
        ],
        "check-start" => &[
            ("shell", true),
            ("root", true),
            ("enable-stdin", false),
            ("project-dir", true),
        ],
        "add-service" => &[("shell", true), ("root", true), ("enable-stdin", false)],
        "restart" => &[
            ("bg", false),
            ("watch", false),
            ("exit-after-ms", true),
            ("project-dir", true),
        ],
        "list" | "ps" => &[("json", false), ("project-dir", true)],
        // list-all is already system-wide, so a project has nothing to say here.
        "list-all" => &[("json", false)],
        "logs" => &[
            ("count", true),
            ("start-at", true),
            ("json", false),
            ("project-dir", true),
        ],
        "watch" => &[("exit-after-ms", true), ("project-dir", true)],
        "wait-for-log" => &[("message", true), ("timeout", true), ("project-dir", true)],
        "list-ports" => &[("json", false), ("project-dir", true)],
        // System-wide, like list-all.
        "list-ports-all" => &[("json", false)],
        "kill" | "clear-logs" | "open-browser" => &[("project-dir", true)],
        "find-orphans" => &[("json", false)],
        "erase-database" => &[("force", false)],
        _ => &[],
    }
}

/// Parsed options for a single command invocation.
#[derive(Debug, Default)]
pub struct CommandArgs {
    pub positionals: Vec<String>,
    pub values: HashMap<String, String>,
    pub bools: HashSet<String>,
}

impl CommandArgs {
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
    pub fn has(&self, name: &str) -> bool {
        self.bools.contains(name)
    }
}

/// Which meta flags (`--help`/`-h`, `--version`/`-v`) appear in a command's arguments.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MetaFlags {
    pub help: bool,
    pub version: bool,
}

/// Find `--help` / `--version` among the tokens following a command, the way
/// [`parse_command_args`] would read them: a token consumed as the value of a
/// value-taking option (`--message --version`) is a value, not a flag, and nothing
/// after a `--` terminator counts. Unknown flags are skipped rather than rejected,
/// so `candle start --bogus --help` still shows help.
pub fn scan_meta_flags(command: &str, tokens: &[String]) -> MetaFlags {
    let spec = option_spec(command);
    let mut out = MetaFlags::default();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        match tok {
            "--" => break,
            "--help" | "-h" => out.help = true,
            "--version" | "-v" => out.version = true,
            _ => {
                if let Some(name) = tok.strip_prefix("--") {
                    let takes_value = !name.contains('=')
                        && spec.iter().any(|(flag, takes)| *flag == name && *takes);
                    if takes_value {
                        i += 1;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// Parse the tokens following a command, enforcing the command's option spec. Returns the
/// `Unknown argument: <flag>` error string on an unrecognized flag, and rejects an inline value
/// on a switch (`--force=false`) rather than silently treating it as set. Everything after a
/// `--` terminator is positional.
pub fn parse_command_args(command: &str, tokens: &[String]) -> Result<CommandArgs, String> {
    let spec = option_spec(command);
    let mut out = CommandArgs::default();
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok == "--" {
            out.positionals.extend(tokens[i + 1..].iter().cloned());
            break;
        }
        if tok == "--help" || tok == "-h" || tok == "--version" || tok == "-v" {
            // Handled by the caller before reaching here, but tolerate it.
            i += 1;
            continue;
        }
        if let Some(rest) = tok.strip_prefix("--") {
            let (name, inline_value) = match rest.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (rest, None),
            };
            match spec.iter().find(|(flag, _)| *flag == name) {
                Some((flag, takes_value)) => {
                    if *takes_value {
                        let value = if let Some(v) = inline_value {
                            v
                        } else if i + 1 < tokens.len() {
                            i += 1;
                            tokens[i].clone()
                        } else {
                            String::new()
                        };
                        out.values.insert((*flag).to_string(), value);
                    } else if inline_value.is_some() {
                        return Err(format!(
                            "Option --{flag} does not take a value (got {tok}). \
                             Pass --{flag} to enable it, or leave it out."
                        ));
                    } else {
                        out.bools.insert((*flag).to_string());
                    }
                }
                None => return Err(format!("Unknown argument: --{name}")),
            }
        } else if tok.starts_with('-') && tok.len() > 1 {
            return Err(format!("Unknown argument: {tok}"));
        } else {
            out.positionals.push(tok.clone());
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve() {
        assert_eq!(canonical_command("run"), Some("start"));
        assert_eq!(canonical_command("ls"), Some("list"));
        assert_eq!(canonical_command("status"), Some("ps"));
        assert_eq!(canonical_command("ps"), Some("ps"));
        assert_eq!(canonical_command("stop"), Some("kill"));
        assert_eq!(canonical_command("bogus"), None);
    }

    #[test]
    fn unknown_flag_errors() {
        let err = parse_command_args("list", &["--bad-flag".to_string()]).unwrap_err();
        assert!(err.contains("Unknown argument"));
    }

    #[test]
    fn unknown_flag_error_keeps_dashes() {
        let err = parse_command_args("list-docs", &["--json".to_string()]).unwrap_err();
        assert_eq!(err, "Unknown argument: --json");
        let err = parse_command_args("list-docs", &["--json=1".to_string()]).unwrap_err();
        assert_eq!(err, "Unknown argument: --json");
        let err = parse_command_args("list", &["-x".to_string()]).unwrap_err();
        assert_eq!(err, "Unknown argument: -x");
    }

    #[test]
    fn value_flag_consumes_following_token_even_if_dashed() {
        let args = parse_command_args(
            "wait-for-log",
            &[
                "echo".to_string(),
                "--message".to_string(),
                "test".to_string(),
                "--timeout".to_string(),
                "-1".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(args.positionals, vec!["echo"]);
        assert_eq!(args.value("message"), Some("test"));
        assert_eq!(args.value("timeout"), Some("-1"));
    }

    #[test]
    fn boolean_flag_recorded() {
        let args = parse_command_args("start", &["svc".to_string(), "--enable-stdin".to_string()])
            .unwrap();
        assert!(args.has("enable-stdin"));
        assert_eq!(args.positionals, vec!["svc"]);
    }

    fn toks(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn boolean_flag_rejects_inline_value() {
        for tok in ["--force=false", "--force=true", "--force="] {
            let err = parse_command_args("erase-database", &toks(&[tok])).unwrap_err();
            assert!(err.contains("does not take a value"), "{tok}: {err}");
        }
        let err = parse_command_args("list", &toks(&["--json=false"])).unwrap_err();
        assert!(err.starts_with("Option --json does not take a value"));
    }

    #[test]
    fn double_dash_ends_options() {
        let args = parse_command_args("start", &toks(&["--bg", "--", "-svc", "--json"])).unwrap();
        assert!(args.has("bg"));
        assert_eq!(args.positionals, vec!["-svc", "--json"]);
    }

    #[test]
    fn meta_flags_skip_option_values() {
        let meta = scan_meta_flags("wait-for-log", &toks(&["api", "--message", "--version"]));
        assert_eq!(meta, MetaFlags::default());
        let meta = scan_meta_flags("wait-for-log", &toks(&["api", "--message", "--help"]));
        assert_eq!(meta, MetaFlags::default());
        let meta = scan_meta_flags("wait-for-log", &toks(&["api", "--message=x", "--help"]));
        assert!(meta.help);
        let meta = scan_meta_flags("start", &toks(&["api", "--bogus", "-h"]));
        assert!(meta.help);
        let meta = scan_meta_flags("list", &toks(&["--version"]));
        assert!(meta.version);
        let meta = scan_meta_flags("start", &toks(&["--", "--help"]));
        assert_eq!(meta, MetaFlags::default());
    }
}
