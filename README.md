# WinRecovery.dll

One clean DLL for the Windows recovery primitives every system tool ends up
re-implementing: **System Restore points, VSS shadow-storage status, the
Recycle Bin, installed programs, startup entries, services, and drivers.**

Built for the tools that need these on day one — uninstallers, backup and
recovery apps, driver managers — and for any language that can call C:
Rust, C/C++, C#, Python (ctypes), Go, AutoHotkey, and more.

## Why this exists

Every Windows system utility re-writes the same fragile glue:

- Shelling out to PowerShell for `Get-ComputerRestorePoint`, then parsing
  its JSON by hand.
- Loading `srclient.dll` at runtime to delete a restore point.
- Declaring `SHQueryRecycleBinW` and its struct correctly — again.
- Walking three different uninstall registry roots (64-bit, WOW6432Node,
  per-user) and deduplicating what comes back.
- The two-call `EnumServicesStatusExW` sizing dance.

WinRecovery does each of these **once**, behind a small, stable C ABI,
with the parsing logic unit-tested off-Windows.

## Highlights

- **Zero dependencies.** The Rust crate declares its Win32 bindings by
  hand; the DLL imports only Windows system libraries and the C runtime.
- **One calling convention for everything.** Enumerations return a JSON
  string (free it with `wr_free_string`); failures return `NULL`/`WR_ERR_*`
  plus a human-readable, thread-local `wr_last_error()`.
- **No panics across the ABI.** Every entry point is wrapped so a bug
  becomes `WR_ERR_INTERNAL`, never a crash in the host process.
- **Honest errors.** Reading is mostly unprivileged; creating/deleting
  restore points needs elevation and Windows' own error is reported —
  nothing is faked or silently skipped.
- **Small.** ~315 KB release build (host build measured; Windows build is
  comparable).

## Quick start (C)

```c
#include "winrecovery.h"
#include <stdio.h>

int main(void) {
    printf("WinRecovery %s\n", wr_version());

    char *json = wr_list_restore_points_json();
    if (!json) {
        fprintf(stderr, "error %d: %s\n", wr_last_error_code(), wr_last_error());
        return 1;
    }
    printf("%s\n", json);
    wr_free_string(json);
    return 0;
}
```

## Quick start (Python, ctypes)

```python
import ctypes, json
wr = ctypes.CDLL("winrecovery.dll")
wr.wr_list_installed_programs_json.restype = ctypes.c_void_p
ptr = wr.wr_list_installed_programs_json()
if not ptr:
    raise RuntimeError(ctypes.c_char_p(wr.wr_last_error()).value.decode())
data = json.loads(ctypes.c_char_p(ptr).value)
wr.wr_free_string(ptr)
print(f"{len(data)} programs installed")
```

## API (v0.1.0)

| Function | Returns |
|---|---|
| `wr_version()` / `wr_version_major/minor/patch()` | static version string / parts (do not free) |
| `wr_last_error()` / `wr_last_error_code()` | thread-local error detail |
| `wr_free_string(p)` | frees any `*_json` result |
| `wr_list_restore_points_json()` | JSON array, newest first |
| `wr_create_restore_point(desc)` | `WR_OK` or error (needs elevation) |
| `wr_delete_restore_point(seq)` | `WR_OK` or error (needs elevation) |
| `wr_restore_point_protection_json()` | enabled / used / max shadow-storage bytes |
| `wr_recycle_bin_info_json(drive_or_null)` | item count + total size |
| `wr_recycle_bin_empty(drive_or_null, no_confirmation)` | `WR_OK` or error |
| `wr_list_installed_programs_json()` | JSON array (3 uninstall roots) |
| `wr_list_startup_entries_json()` | Run/RunOnce + Startup folders |
| `wr_list_services_json()` | services + drivers with state/PID |
| `wr_list_drivers_json()` | drivers only |

Full shapes and error codes: [`include/winrecovery.h`](include/winrecovery.h)
and [`docs/API.md`](docs/API.md).

## Build

```sh
cargo build --release          # produces the cdylib for the host target
cargo test                     # pure-logic tests run on any OS
cargo check --target x86_64-pc-windows-msvc --all-targets
```

Windows release artifacts (`winrecovery.dll` + import library) are built by
CI on `windows-latest`.

## Verification status

- Unit tests (parsing, JSON shapes, FFI error contract): **9/9 passing** on Linux.
- `cargo clippy --all-targets -- -D warnings`: clean.
- Windows target compile (`x86_64-pc-windows-msvc`, all targets): **passes**.
- C consumer smoke test against the built library: **passes** (version +
  error contract).
- **Not yet verified:** running the Windows build on a real Windows machine
  (restore-point create/list/delete, Recycle Bin, SCM enumeration). That
  on-machine pass is the v0.1.0 release gate — see `docs/API.md`.

## The DLL Collection

WinRecovery is #1 of a planned set of eight single-purpose Windows DLLs —
see [`ROADMAP.md`](ROADMAP.md). Next up: **FastSearch.dll** (instant NTFS
file search).

## License

MIT — see [`LICENSE`](LICENSE).
