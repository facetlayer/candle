// candle — primary CLI binary.
//
// M0 scaffold: only `--version` / `-v` is wired so the workspace builds and produces the `candle`
// binary. All commands are reimplemented in later milestones (see ../../PORTING_PLAN.md) on top of
// the `candle_core` library.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    eprintln!("candle (rust): not yet implemented");
    std::process::exit(1);
}
