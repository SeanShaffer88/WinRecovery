# WinRecovery — agent notes

- Zero dependencies is a design rule: Win32 bindings are hand-declared in the `mod windows` inside src/platform.rs (cfg(windows)-gated). Do not add windows-sys or any crate without a written justification in the PR.
- Pure logic (parsers, JSON) lives in src/lib.rs and must stay testable off-Windows. Platform calls on non-Windows return WR_ERR_NOT_SUPPORTED.
- Gate: cargo fmt --all, cargo clippy --all-targets -- -D warnings, cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings (host clippy never sees the cfg(windows) code — CI run #1 failed on exactly that), cargo test (full suite), cargo check --target x86_64-pc-windows-msvc --all-targets.
- Never claim Windows runtime behavior is verified until it has run on a real Windows machine; compile-verification is not runtime verification.
- Documentation names no third-party products beyond components actually called (PowerShell cmdlets, Windows DLLs).

---

## CI notes (learned the hard way)

- Both jobs are part of the gate. On `windows-latest` the host triple already *is* `x86_64-pc-windows-msvc`, so `clippy --target x86_64-pc-windows-msvc` there is a no-op that proves nothing. The `cross` job on `ubuntu-latest` is what actually enforces the cross-target gate. Do not delete it as "redundant".
- `windows-latest` ships **Visual Studio 18 (2026) Enterprise** (confirmed Oct 2026). Never hardcode a VS version path — a pinned `...\Microsoft Visual Studio\2022\Enterprise` broke the build when the image moved. Discover the install with `vswhere`, then probe for `VC\Auxiliary\Build\vcvars64.bat`.
- Do **not** filter vswhere with `-requires VC.Tools.x86.x64`. That component ID is not registered on VS 18 even though the MSVC toolchain is present and the Rust MSVC target links fine, so the filter selects nothing. Test for `vcvars64.bat` instead.
- Do **not** pass a quoted command string from `pwsh` into `cmd /c`. PowerShell 7.3+ defaults to `Standard` native argument passing and escapes inner quotes as `\"`, which `cmd` does not understand; the command fails with exit 1 and **no output**, which reads like a compiler error but is not. Write a batch file to `$env:RUNNER_TEMP` and invoke that.
- Build C through `vcvars64.bat`, not the DevShell PowerShell module: it is the documented MSVC entry point and exists in every VS layout (2022 and 2026).
- CI logs are not readable from every development environment: raw job logs are served from `results-receiver.actions.githubusercontent.com`, artifact bytes from `productionresultssa*.blob.core.windows.net`, and job summaries are not exposed through the REST API (`check-runs` `output.summary` is null). Any step that can fail opaquely should post its own captured output to the PR — see `Report-Failure` in the MSVC step. That is why the workflow holds `pull-requests: write`.
- Never suppress output on a command that might fail (`>nul 2>&1`). It is what made the mangled `cmd /c` failure look like a compiler bug for several runs.

---

## Development standard

This project follows SEAN's **Universal Windows Desktop Master Prompt** — the standing development standard for all of SEAN's Windows desktop projects, current and future. The full text is maintained by SEAN (ask him for the current version); this section is the in-repo summary, and the project's own instructions add project-specific requirements on top of it.

Core standards:

- Production quality only: no rough demos or placeholder features presented as finished; safe defaults for beginners, explained advanced controls for professionals.
- Accessibility is a requirement: keyboard navigation, accessible names, visible focus, sufficient contrast, no meaning by color alone, specific error messages with a fix path.
- Every consequential action explains its risks, limitations, and recovery options; destructive or irreversible actions are confirmed, previewed where feasible, and backed up or undoable where feasible.
- Data safety: local-first storage, offline-capable core features, versioned data formats, atomic writes, tested migrations, and no silent data loss.
- Verification honesty: compatibility and readiness claims are labeled Verified, Partially verified, or Unverified; never claim a check that did not run.

Standing defaults (a project brief may override a specific default):

- Product-family look: the black/red aesthetic matching InstallScope is preferred, not mandatory.
- Rust is the default for new projects and for new components where it fits; existing stacks are preserved — this is not a rewrite mandate.
- Support as many Windows versions as makes sense for the project; consider Windows 10 support when feasible without materially extending development time. Never claim support that has not been verified.
- Status and task reports include a Progress % and an ETA whenever they can be estimated responsibly.
- Public documentation avoids competitor comparisons by default (internal research may name competitors; license attributions for bundled components are kept).
- License: MIT for SEAN-owned projects; forks retain their upstream license.
