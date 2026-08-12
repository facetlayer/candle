// Hand-rolled argument parser for the candle CLI.
//
// We deliberately do not use clap: the Vitest suite asserts on yargs-specific behavior — the literal
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
        "list" | "ls" | "status" => "list",
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
        ],
        "check-start" | "add-service" => {
            &[("shell", true), ("root", true), ("enable-stdin", false)]
        }
        "restart" => &[("bg", false), ("watch", false), ("exit-after-ms", true)],
        "list" | "list-all" => &[("json", false)],
        "logs" => &[("count", true), ("start-at", true)],
        "watch" => &[("exit-after-ms", true)],
        "wait-for-log" => &[("message", true), ("timeout", true)],
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

/// Parse the tokens following a command, enforcing the command's option spec. Returns the
/// yargs-style `Unknown argument: <flag>` error string on an unrecognized flag.
pub fn parse_command_args(command: &str, tokens: &[String]) -> Result<CommandArgs, String> {
    let spec = option_spec(command);
    let mut out = CommandArgs::default();
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok == "--help" || tok == "-h" {
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
                    } else {
                        out.bools.insert((*flag).to_string());
                    }
                }
                None => return Err(format!("Unknown argument: {name}")),
            }
        } else if tok.starts_with('-') && tok.len() > 1 {
            return Err(format!("Unknown argument: {}", &tok[1..]));
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
        assert_eq!(canonical_command("status"), Some("list"));
        assert_eq!(canonical_command("stop"), Some("kill"));
        assert_eq!(canonical_command("bogus"), None);
    }

    #[test]
    fn unknown_flag_errors() {
        let err = parse_command_args("list", &["--bad-flag".to_string()]).unwrap_err();
        assert!(err.contains("Unknown argument"));
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
        let args =
            parse_command_args("start", &["svc".to_string(), "--enable-stdin".to_string()]).unwrap();
        assert!(args.has("enable-stdin"));
        assert_eq!(args.positionals, vec!["svc"]);
    }
}
