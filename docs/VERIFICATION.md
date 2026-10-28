# WinRecovery — on-machine verification record

This is the record for WinRecovery v0.1.0's release gate. CI proves the ABI
contract holds; **only a run on a real Windows machine proves the library
returns correct data**. Until the matrix below is filled in from an actual
run, v0.1.0 stays an untagged foundation build.

Status labels follow the project standard:

| Label | Meaning |
|---|---|
| **Verified** | Ran on a real machine and the result was cross-checked against what Windows itself reports. |
| **Partially verified** | Ran, but not cross-checked, or blocked by the environment (not elevated, System Protection off). |
| **Unverified** | Has not run. |

A call returning JSON is **not** verification. Each check below names the
Windows source to compare against, because the point of this gate is data
correctness, not absence of crashes.

## Current status

**Unverified — no on-machine run has been recorded.**

Everything in the matrix below is a template. Fill in the Result and Evidence
columns from a real run and change the status line above.

---

## How to run

Get the DLL — either build it on Windows:

```sh
cargo build --release
```

or download the `winrecovery-windows-x64` artifact from any CI run's summary
page (it contains `winrecovery.dll`, its import library, and
`winrecovery.h`).

Then, from an **elevated** PowerShell so the mutation checks can run:

```powershell
cd examples\powershell

# Read-only pass. Safe: changes nothing.
.\smoke-test.ps1 -DllPath C:\path\to\winrecovery.dll -ReportPath .\results.md

# Full pass. Creates and then deletes one restore point (self-cleaning).
.\smoke-test.ps1 -DllPath C:\path\to\winrecovery.dll -IncludeMutations -ReportPath .\results.md
```

The script prints `PASS` / `FAIL` / `BLOCKED` / `SKIP` per check and ends with
a verdict:

- **FULLY VERIFIED** — every read-only and mutation check passed. The gate is
  closed; record it below.
- **READ-ONLY VERIFIED** — reads are good but the mutations did not run. The
  gate is **not** closed.
- **PARTIALLY VERIFIED** — something was blocked or skipped.
- **NOT VERIFIED** — at least one failure.

`-ReportPath` writes the same results as markdown, ready to paste into the
table below.

### `BLOCKED` is not `FAIL`

`BLOCKED` means the check could not run for an environmental reason — not
elevated, or System Protection is disabled on the machine. That is Windows
behaving correctly, not a library defect. Resolve the environment and re-run
rather than recording it as a failure.

---

## Environment

Record this for every run. Results are only meaningful alongside the exact
Windows build they came from.

| | |
|---|---|
| Date | |
| Windows edition | |
| Build (e.g. `10.0.26100`) | |
| Architecture | |
| Library version (`wr_version`) | |
| DLL source (local build / CI artifact + run URL) | |
| Elevated | |
| System Protection enabled on the system drive | |
| `-IncludeMutations` used | |
| `-EmptyRecycleBin` used | |

---

## Matrix

### Read-only

| Function | Cross-check against | Result | Evidence |
|---|---|---|---|
| `wr_version` + `wr_version_major/minor/patch` | `Cargo.toml` version; the parts must equal the string | | |
| `wr_last_error` / `wr_last_error_code` | A deliberately failing call reports a code in range and a non-empty message | | |
| `wr_free_string` | No crash and no leak across many calls (the script frees every result) | | |
| `wr_list_restore_points_json` | `Get-ComputerRestorePoint` — same sequence numbers, descriptions, dates | | |
| `wr_restore_point_protection_json` | `vssadmin list shadowstorage` — used/max bytes match; System Protection dialog for `enabled` | | |
| `wr_recycle_bin_info_json` (all drives) | Recycle Bin properties in Explorer — item count and size | | |
| `wr_recycle_bin_info_json("C:\")` | Same, for that drive only; must differ from the all-drives figure when other drives hold items | | |
| `wr_list_installed_programs_json` | Settings → Apps → Installed apps. Check counts, and that 32-bit entries appear under `machine32` and per-user ones under `user` | | |
| `wr_list_startup_entries_json` | Task Manager → Startup tab, plus the Run keys in `regedit` | | |
| `wr_list_services_json` | `services.msc` — names, display names, states, PIDs | | |
| `wr_list_drivers_json` | Must equal the `is_driver: true` subset of `wr_list_services_json` (the script asserts this) | | |

### UTF-8 correctness

The library returns UTF-8 and accepts UTF-8. Windows PowerShell's default
ANSI decoding hides mistakes here, so check it explicitly.

| Check | What to look for | Result | Evidence |
|---|---|---|---|
| Non-ASCII installed programs | Any program name or publisher with accents, CJK, or symbols must render correctly, not as `?` or mojibake | | |
| Non-ASCII restore-point description (`-IncludeMutations`) | The script creates a point whose description contains `é ü € 中` and asserts it reads back byte-for-byte after create → registry → enumerate → JSON → decode | | |

If the machine has no non-ASCII program names, the installed-program check
reports `SKIP` — the restore-point check covers UTF-8 either way.

### Mutations (require elevation)

| Function | What a pass means | Result | Evidence |
|---|---|---|---|
| `wr_create_restore_point` | Returns `WR_OK`; the point then appears in `wr_list_restore_points_json` **and** in `Get-ComputerRestorePoint` | | |
| Restore-point ordering | The list is newest-sequence first, as documented | | |
| `wr_delete_restore_point` | Returns `WR_OK` for the sequence just created; that sequence is gone from both `wr_list_restore_points_json` and `Get-ComputerRestorePoint` | | |
| Description sanitisation | The created description matches the documented filter (alphanumerics and ` -_.()`, truncated at 60 characters). Note `€` is a symbol, not alphanumeric, so it is correctly removed | | |
| Registry side-effect reverted | `SystemRestorePointCreationFrequency` under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore` is back to its pre-run value — the library sets it to 0 temporarily and must restore it | | |
| Cleanup left nothing behind | No `WRVERIFY-*` restore point survives the run | | |

The create/delete pair is self-cleaning by design: the script deletes exactly
the point it created, and does so in a `finally` block so an interrupted run
still cleans up. If it ever cannot identify the point, it fails loudly and
tells you to delete it manually rather than silently leaving residue.

### Destructive — `-EmptyRecycleBin`

**Only run this on a machine whose Recycle Bin you do not care about. It is
irreversible.** It is a separate switch and is never implied by
`-IncludeMutations`. The script previews the item count and size and requires
you to type `EMPTY` before anything is deleted; if input is redirected it
refuses rather than destroying data unattended.

| Check | What a pass means | Result | Evidence |
|---|---|---|---|
| `wr_recycle_bin_empty` | Returns `WR_OK`, and `wr_recycle_bin_info_json` then reports 0 items and 0 bytes for the same target | | |

### Error paths

Worth recording, since honest errors are a design claim of this library.

| Check | Expected | Result | Evidence |
|---|---|---|---|
| Create/delete restore point without elevation | A `WR_ERR_*` code and a message naming the real Windows failure — not a crash, not silent success | | |
| `wr_recycle_bin_info_json` on a drive with no Recycle Bin (e.g. a drive letter that does not exist) | A clean error, not a crash | | |
| Restore-point operations on an edition without System Restore | `WR_ERR_PLATFORM` with a clear message, since `srclient.dll` is loaded at runtime | | |

---

## Sign-off

Record this only when the matrix above is filled in from a real run and every
read-only **and** mutation check passed.

| | |
|---|---|
| Verified by | |
| Date | |
| Verdict from the script | |
| Failures / caveats | |
| v0.1.0 tagged? | |
