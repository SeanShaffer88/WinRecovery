# WinRecovery — agent notes

- Zero dependencies is a design rule: Win32 bindings are hand-declared in src/platform/windows.rs. Do not add windows-sys or any crate without a written justification in the PR.
- Pure logic (parsers, JSON) lives in src/lib.rs and must stay testable off-Windows. Platform calls on non-Windows return WR_ERR_NOT_SUPPORTED.
- Gate: cargo fmt --all, cargo clippy --all-targets -- -D warnings, cargo test (full suite), cargo check --target x86_64-pc-windows-msvc --all-targets.
- Never claim Windows runtime behavior is verified until it has run on a real Windows machine; compile-verification is not runtime verification.
- Documentation names no third-party products beyond components actually called (PowerShell cmdlets, Windows DLLs).
