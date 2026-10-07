//! `rustle`: the name Wardian had before the rename. Kept so existing scripts and
//! habits keep working; it runs the `wardian` program next to it with the same arguments.

use std::process::{exit, Command};

fn main() {
    let exe = std::env::current_exe().ok().map(|p| p.with_file_name(if cfg!(windows) { "wardian.exe" } else { "wardian" }));
    let Some(exe) = exe.filter(|p| p.exists()) else {
        eprintln!("rustle is now called wardian, but the wardian program is not next to this one. Run wardian instead.");
        exit(1);
    };
    match Command::new(&exe).args(std::env::args_os().skip(1)).status() {
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("rustle: cannot start {}: {e}", exe.display());
            exit(1);
        }
    }
}
