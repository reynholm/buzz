//! Platform entry points for the signed blue/green deployment verifier.
//! Unix permission checks protect its key-bearing configuration and ledger.

#[cfg(unix)]
#[path = "blue_green_probe/unix.rs"]
mod unix;

#[cfg(unix)]
fn main() -> anyhow::Result<()> {
    unix::main()
}

#[cfg(not(unix))]
fn main() {
    eprintln!("blue_green_probe requires Unix file-permission support");
    std::process::exit(1);
}
