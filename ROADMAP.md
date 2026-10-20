# Roadmap — The Windows DLL Collection

Eight single-purpose, zero-dependency-style DLLs with clean C ABIs, each
solving one painful Windows problem once, for every language that can call C.

| # | Library | Purpose | Status |
|---|---|---|---|
| 1 | **WinRecovery.dll** | Restore points, VSS status, Recycle Bin, installed programs, startup, services/drivers | **v0.1.0 built** — on-machine Windows verification is the release gate |
| 2 | **FastSearch.dll** | Instant file search via NTFS USN Journal + MFT (`fs_search()` in milliseconds) | Next |
| 3 | **ArchiveKit.dll** | ZIP / 7z / TAR / ZST / GZ open, extract, create with progress + cancellation | Planned |
| 4 | **SQLite SuperBuild** | Drop-in `sqlite3.dll` + FTS5 + JSON + R*Tree + sqlite-vec vector search | Planned |
| 5 | **Whisper.dll** | Local speech-to-text (whisper.cpp) behind one `whisper_transcribe()` call | Planned |
| 6 | **ImageKit.dll** | HEIC / AVIF / WebP / JXL decode, convert, thumbnails | Planned |
| 7 | **WinUI Helpers.dll** | Win11 toast notifications, tray icon, global hotkeys from any language | Planned |
| 8 | **HardwareInfo.dll** | CPU/GPU temperature, load, fan, VRAM, disk SMART health in one call | Planned |

Shared design rules for all eight:

- Rust `cdylib`, C ABI, UTF-8 strings, JSON for structured results.
- Errors as codes + a thread-local human message; never panic across the ABI.
- Minimal dependencies, each justified; small single-file DLLs.
- Pure logic unit-tested off-Windows; Windows behavior verified on a real
  machine before any release claims it works.
- MIT licensed.

## WinRecovery v0.2 candidates

- VSS snapshot enumeration (`vssadmin list shadows` successor API in-process)
- Restore-point rollback trigger (`wr_restore_to`)
- System Protection enable/disable + max-space setters
- Scheduled-task enumeration alongside startup entries
- Driver Store (pnputil-style) enumeration complementing the SCM driver list
