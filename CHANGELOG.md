# Changelog

All notable changes to WinRecovery are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses semantic versioning.

The C ABI is the contract: anything under `include/winrecovery.h` that is
already shipped stays source- and binary-compatible until a major version
bump. Additive changes (new `wr_*` entry points) are minor bumps.

## [Unreleased]

### Added

- CI cross-target gate job on `ubuntu-latest`: the Windows-target clippy and
  check that `AGENTS.md` mandates now actually run from a non-Windows host.
  On `windows-latest` the host triple already is `x86_64-pc-windows-msvc`, so
  those steps were previously no-ops that proved nothing.
- C consumer contract test (`examples/c/contract.c`), compiled with MSVC
  against the built import library and run in CI. Asserts, from the C side of
  the ABI, that every read-only entry point either returns valid JSON or
  fails with a `WR_ERR_*` code plus a non-empty `wr_last_error()`; that NULL
  and non-UTF-8 arguments yield `WR_ERR_INVALID_ARG`; that the initial
  last-error state is `WR_OK`; and that `wr_free_string(NULL)` is a no-op.
  Machine-independent by design — a runner with System Restore off is not a
  library defect.
- CI artifact `winrecovery-windows-x64` containing `winrecovery.dll`,
  `winrecovery.dll.lib`, and `winrecovery.h`, so the DLL is downloadable
  without a local Windows toolchain. `examples/powershell/smoke-test.ps1`
  already pointed at CI artifacts that did not exist.
- `workflow_dispatch` trigger on CI, so a build can be produced for
  on-machine verification without a throwaway commit.
- A "CI notes (learned the hard way)" section in `AGENTS.md`, covering Visual
  Studio discovery on `windows-latest`, the `pwsh`-to-`cmd` quoting trap, and
  the fact that CI logs, artifacts, and job summaries are not reachable from
  every development environment.
- `docs/VERIFICATION.md`: the on-machine verification record. An environment
  table, then a per-function matrix naming the Windows source each result
  must be cross-checked against (`Get-ComputerRestorePoint`, `vssadmin list
  shadowstorage`, `services.msc`, Task Manager's Startup tab, Explorer's
  Recycle Bin properties), plus separate UTF-8, mutation, destructive and
  error-path sections and a sign-off block. A call returning JSON is not
  verification, so each row states what correct looks like.
- CI parse-checks every `.ps1` under `examples/powershell` on
  `windows-latest` via the PowerShell AST parser. Parse-only: it executes
  nothing and needs no DLL, so it cannot mutate a runner. A syntax error in
  the verification script would otherwise surface only on the one machine
  someone finally runs it on.
- A "PowerShell notes (for verification scripts)" section in `AGENTS.md`.
  Every DLL in the collection needs an on-machine verification script, so the
  traps found here are recorded for reuse: `PtrToStringAnsi` corrupting UTF-8,
  `AllocHGlobal` buffers never going to `wr_free_string`, `-contains` on a
  scalar string always being false, `[string]` parameters binding `$null` as
  `""`, BOM-less `.ps1` files being read as ANSI by PowerShell 5.1, and
  keeping `BLOCKED` distinct from `FAIL`.

### Changed

- `wr_version()` is now derived from `crate::VERSION` at compile time instead
  of a second hardcoded version literal that could drift from it, and the
  `version_exports` test now also asserts the NUL terminator sits exactly past
  the string (a truncating derivation would otherwise still read back as a
  shorter-but-valid string).
- Removed the root `ci.yml`. It was a byte-identical copy of the live
  workflow left over from when GitHub required workflows to be added through
  the web UI; two copies of a gate is a way to end up with neither enforced.
- `examples/powershell/smoke-test.ps1` grew from a read-only smoke test into
  the gate's full verification pass. It reports `PASS` / `FAIL` / `BLOCKED` /
  `SKIP` per check with a closing verdict, detects elevation, cross-checks
  `wr_version` against its `_major`/`_minor`/`_patch` parts, queries the
  Recycle Bin for a specific drive as well as all drives, asserts that
  `wr_list_drivers_json` equals the `is_driver` subset of
  `wr_list_services_json`, and writes markdown via `-ReportPath`.
- Two opt-in phases replace the read-only-only scope. `-IncludeMutations`
  creates one restore point whose description contains `é ü € 中`, verifies it
  appears and reads back unchanged, then deletes exactly that point. It is
  self-cleaning, the delete sits in a `finally` block so an interrupted run
  cannot strand a restore point, and it recovers if the created point could
  not be identified. `-EmptyRecycleBin` is separate and never implied by
  `-IncludeMutations`: it previews item count and size, requires typing
  `EMPTY`, and refuses when input is redirected.
- `BLOCKED` is now distinguished from `FAIL`. A check that could not run
  because the shell was not elevated, or because System Protection was off,
  is Windows behaving correctly rather than a library defect. Without that
  distinction an environmental limit reads as a bug, and a skipped check
  reads as a verified one.
- The P/Invoke type name is unique per run and the compiled type is captured
  with `Add-Type -PassThru`, so re-running in one session, or against a second
  DLL, no longer fails with "type already exists".

### Fixed

- `smoke-test.ps1` decoded the library's UTF-8 strings with
  `Marshal.PtrToStringAnsi`, which uses the process ANSI code page and so
  corrupted every non-ASCII byte sequence — accented program names, CJK
  publishers, curly quotes. The verification tool could not read the UTF-8
  contract it existed to check, and would have reported mangled data as a
  pass. Replaced with an explicit NUL scan plus `Encoding.UTF8.GetString`,
  because `Marshal.PtrToStringUTF8` does not exist on .NET Framework
  (Windows PowerShell 5.1).
- Incoming strings are now allocated as UTF-8 and released with
  `Marshal.FreeHGlobal`, never `wr_free_string`: that buffer came from
  `AllocHGlobal`, so handing it to the library's allocator would mismatch the
  heap.
- The description filter in the mutation check used `' -_.()' -contains $_`,
  which is always false — `-contains` compares against a collection's
  elements, and a scalar string is a one-element collection whose only
  element is the whole string. Spaces, hyphens, underscores, dots and parens
  were therefore stripped from the *expected* value, so a correct round trip
  would have been reported as a mismatch.
- `AGENTS.md` referenced `src/platform/windows.rs`; the Windows bindings live
  in the `mod windows` inside `src/platform.rs`.
- MSVC discovery on `windows-latest`. The image now ships Visual Studio 18
  (2026) Enterprise, so the step's hardcoded `...\2022\Enterprise` path no
  longer exists. The step now enumerates installs with `vswhere` and probes
  each for `VC\Auxiliary\Build\vcvars64.bat`, which survives future image
  moves. Filtering `vswhere` on `-requires VC.Tools.x86.x64` was also tried
  and does not work: that component ID is not registered on VS 18 even though
  the toolchain is present and the Rust MSVC target links against it.
- The C build no longer passes a quoted command string from `pwsh` into
  `cmd /c`. PowerShell 7.3+ escapes inner quotes as `\"`, which `cmd` does not
  understand, so the compiler never ran and the step failed with exit 1 and no
  output — indistinguishable from a compile error. It now writes a batch file
  to `$env:RUNNER_TEMP` with distinct exit codes per phase (1 `vcvars`,
  2 `contract.c`, 3 `example.c`) and stops suppressing `vcvars` output.
- The MSVC step reports its own captured output to the pull request when it
  fails, because the failure was otherwise undiagnosable: raw logs, artifact
  bytes, and job summaries are all unreachable from restricted environments.
  An earlier revision declared that reporter *after* the discovery code that
  could throw, so setup failures still exited silently.

## [0.1.0] — unreleased foundation build

Initial API. Not tagged: v0.1.0's own release gate — running the
enumeration and mutation calls against a live Windows machine's real data —
has not been closed, so this version stays a foundation build. See the
verification status in `README.md` for exactly what is and is not verified.

### Added

- System Restore: `wr_list_restore_points_json`, `wr_create_restore_point`,
  `wr_delete_restore_point`, `wr_restore_point_protection_json`.
- Recycle Bin: `wr_recycle_bin_info_json`, `wr_recycle_bin_empty`.
- Enumeration: `wr_list_installed_programs_json` (three uninstall roots,
  deduplicated), `wr_list_startup_entries_json` (Run/RunOnce plus Startup
  folders), `wr_list_services_json`, `wr_list_drivers_json`.
- ABI plumbing: `wr_version` and its `_major`/`_minor`/`_patch` parts,
  `wr_last_error`, `wr_last_error_code`, `wr_free_string`.
- Structured results as JSON strings; errors as codes plus a thread-local
  human message; every entry point wrapped so a bug becomes
  `WR_ERR_INTERNAL` rather than a panic across the ABI.
- Zero-dependency design: Win32 bindings hand-declared, no `windows-sys`.
- Pure parsing logic unit-tested off-Windows, `include/winrecovery.h`,
  `docs/API.md`, a C example, and a read-only PowerShell smoke test for the
  on-machine verification pass.

[Unreleased]: https://github.com/SeanShaffer88/WinRecovery/commits/main
[0.1.0]: https://github.com/SeanShaffer88/WinRecovery
