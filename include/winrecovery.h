/*
 * WinRecovery.dll — C ABI
 * =======================
 * One clean DLL for Windows recovery primitives: System Restore points,
 * VSS shadow-storage status, the Recycle Bin, installed programs,
 * startup entries, services, and drivers.
 *
 * Conventions
 * -----------
 * - All strings are UTF-8, NUL-terminated.
 * - Functions returning `char*` transfer ownership to the caller; release
 *   the string with wr_free_string(). On failure they return NULL —
 *   call wr_last_error() / wr_last_error_code() for details.
 * - wr_last_error() returns a thread-local message owned by the library.
 *   Do NOT free it. It stays valid until the next WinRecovery call made
 *   on the same thread.
 * - Functions returning int return WR_OK (0) on success or a WR_ERR_* code.
 * - Most operations need no elevation to READ. Creating/deleting restore
 *   points and emptying another drive's Recycle Bin generally require
 *   administrator rights; Windows enforces this, and the resulting error
 *   is reported through wr_last_error().
 *
 * Version: 0.1.0 — see wr_version().
 */

#ifndef WINRECOVERY_H
#define WINRECOVERY_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32) && defined(WINRECOVERY_BUILD)
#define WR_API __declspec(dllexport)
#elif defined(_WIN32)
#define WR_API __declspec(dllimport)
#else
#define WR_API
#endif

/* Error codes */
#define WR_OK                   0
#define WR_ERR_INVALID_ARG      1
#define WR_ERR_NOT_SUPPORTED    2  /* not on this platform / edition        */
#define WR_ERR_NOT_FOUND        3
#define WR_ERR_ACCESS_DENIED    4
#define WR_ERR_PLATFORM         5  /* Windows API / PowerShell failure      */
#define WR_ERR_INTERNAL         6

/* Version & errors ------------------------------------------------------- */

/* Static string. Do NOT free. */
WR_API const char *wr_version(void);
WR_API uint32_t wr_version_major(void);
WR_API uint32_t wr_version_minor(void);
WR_API uint32_t wr_version_patch(void);

/* Thread-local message owned by the library. Do NOT free. */
WR_API const char *wr_last_error(void);
WR_API int wr_last_error_code(void);

/* Free a string returned by any *_json function below. NULL is a no-op. */
WR_API void wr_free_string(char *p);

/* System Restore ---------------------------------------------------------- */

/* JSON array: [{"sequence":7,"description":"...","created":"2026-10-15 08:30",
 *               "kind":"manual","kind_code":12}, ...] newest first. */
WR_API char *wr_list_restore_points_json(void);

/* Create a restore point. Requires elevation. */
WR_API int wr_create_restore_point(const char *description_utf8);

/* Delete the restore point with this sequence number. Requires elevation. */
WR_API int wr_delete_restore_point(uint32_t sequence);

/* JSON object: {"enabled":true,"drive":"C:","used_bytes":2480000000,
 *               "max_bytes":25500000000}  (byte fields may be null) */
WR_API char *wr_restore_point_protection_json(void);

/* Recycle Bin -------------------------------------------------------------- */

/* drive: "C:\\" style root, or NULL for all drives together.
 * JSON object: {"drive":"C:\\","item_count":12,"size_bytes":1048576} */
WR_API char *wr_recycle_bin_info_json(const char *drive_or_null);

/* Empty the Recycle Bin. no_confirmation != 0 suppresses the Windows prompt. */
WR_API int wr_recycle_bin_empty(const char *drive_or_null, int no_confirmation);

/* Enumeration --------------------------------------------------------------- */

/* JSON array of installed programs from the classic uninstall registry
 * roots (machine 64-bit, machine 32-bit/WOW6432Node, current user):
 * [{"key":"...","name":"...","publisher":"...","version":"...",
 *   "install_date":"2026-10-15","install_location":"...","uninstall_string":"...",
 *   "size_kb":12345,"source":"machine64","system_component":false}, ...] */
WR_API char *wr_list_installed_programs_json(void);

/* JSON array of startup entries from Run/RunOnce (HKLM+HKCU) and the
 * per-user and common Startup folders:
 * [{"name":"...","command":"...","location":"...","source":"registry"}, ...] */
WR_API char *wr_list_startup_entries_json(void);

/* JSON array of Win32 services AND drivers:
 * [{"name":"...","display_name":"...","state":"running",
 *   "process_id":1234,"is_driver":false}, ...] */
WR_API char *wr_list_services_json(void);

/* Same shape as wr_list_services_json(), drivers only. */
WR_API char *wr_list_drivers_json(void);

#ifdef __cplusplus
}
#endif

#endif /* WINRECOVERY_H */
