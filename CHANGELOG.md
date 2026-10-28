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

### Changed

- `wr_version()` is now derived from `crate::VERSION` at compile time instead
  of a second hardcoded version literal that could drift from it, and the
  `version_exports` test now also asserts the NUL terminator sits exactly past
  the string (a truncating derivation would otherwise still read back as a
  shorter-but-valid string).
- Removed the root `ci.yml`. It was a byte-identical copy of the live
  workflow left over from when GitHub required workflows to be added through
  the web UI; two copies of a gate is a way to end up with neither enforced.

### Fixed

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
