# WinRecovery API — v0.1.0

Authoritative declaration: [`../include/winrecovery.h`](../include/winrecovery.h).
This document explains semantics, JSON shapes, and known limitations.

## Ownership & errors

- `*_json` functions return a heap string owned by the **caller**. Free it
  exactly once with `wr_free_string`. `NULL` means failure.
- `wr_version` and `wr_last_error` return library-owned strings. Never free.
- Integer functions return `WR_OK` (0) or:
  `WR_ERR_INVALID_ARG` (1), `WR_ERR_NOT_SUPPORTED` (2), `WR_ERR_NOT_FOUND` (3),
  `WR_ERR_ACCESS_DENIED` (4), `WR_ERR_PLATFORM` (5), `WR_ERR_INTERNAL` (6).
- After any failure, `wr_last_error()` gives a human message (thread-local,
  valid until the next WinRecovery call on that thread).

## JSON shapes

Restore point:
```json
{"sequence":7,"description":"Before uninstall","created":"2026-10-15 08:30","kind":"manual","kind_code":12}
```
`kind` is one of `install`, `uninstall`, `manual`, `update`, `system`
(from the Windows restore-point type code in `kind_code`). Lists are
newest-sequence first.

Protection:
```json
{"enabled":true,"drive":"C:","used_bytes":2480000000,"max_bytes":25500000000}
```
`used_bytes`/`max_bytes` are `null` when `vssadmin` reports no shadow
storage for the drive.

Recycle Bin:
```json
{"drive":"C:\\","item_count":12,"size_bytes":1048576}
```
An empty `drive` string means the query ran for all drives together
(the `drive_or_null` argument was `NULL`).

Installed program:
```json
{"key":"{GUID}","name":"Example App","publisher":"Example Co","version":"1.2.3","install_date":"2026-10-15","install_location":"C:\\Program Files\\Example","uninstall_string":"...","size_kb":12345,"source":"machine64","system_component":false}
```
Sources: `machine64` (HKLM 64-bit view), `machine32` (WOW6432Node),
`user` (HKCU). Entries without a `DisplayName` are skipped. Store/UWP apps
are **not** included in v0.1 (classic uninstall roots only).

Startup entry:
```json
{"name":"Example","command":"C:\\Apps\\example.exe /min","location":"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run","source":"registry"}
```
`source` is `registry` (Run/RunOnce, HKLM+HKCU) or `folder` (Startup folders).

Service / driver:
```json
{"name":"ExampleSvc","display_name":"Example Service","state":"running","process_id":1234,"is_driver":false}
```
States: `stopped`, `start_pending`, `stop_pending`, `running`,
`continue_pending`, `pause_pending`, `paused`, `unknown`.

## Platform notes & limitations (honest list)

- Restore-point list/create go through PowerShell
  (`Get-ComputerRestorePoint` / `Checkpoint-Computer`) — the same proven
  approach InstallScope uses. Create temporarily sets
  `SystemRestorePointCreationFrequency` to 0 and restores the old value.
- Delete loads `srclient.dll` at runtime and calls `SRRemoveRestorePoint`;
  editions without it get a clean `WR_ERR_PLATFORM`, not a load failure.
- Create/delete require elevation; the library never prompts for it.
- All Windows behavior is compile-verified for `x86_64-pc-windows-msvc`
  but **not yet run on a real Windows machine**. Treat v0.1.0 as a
  foundation build until the on-machine matrix in the README is checked off.
- Non-Windows builds exist so the parsing/FFI logic can be tested anywhere;
  every platform call there returns `WR_ERR_NOT_SUPPORTED`.
