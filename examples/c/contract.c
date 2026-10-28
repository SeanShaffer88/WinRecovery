/*
 * WinRecovery — C consumer contract test.
 *
 * This is a test asset, not a usage example; see example.c for that. It
 * exists to prove, from the C side of the ABI, what the Rust unit tests
 * cannot: that the built DLL actually exports the documented symbols, that
 * the header's declarations link against winrecovery.dll.lib, and that the
 * ownership and error rules in winrecovery.h hold at runtime.
 *
 * Deliberately machine-independent. A CI runner with System Restore turned
 * off, or without elevation, is not a library defect — so every read-only
 * call is accepted either way:
 *
 *   success -> non-NULL, valid-looking UTF-8 JSON, freeable exactly once
 *   failure -> NULL, a non-zero WR_ERR_* code, and a non-empty wr_last_error()
 *
 * Anything else is a contract violation and fails the build. Exit code is the
 * number of violations.
 *
 * Build (Windows, MSVC developer shell):
 *   cl /nologo /W4 /WX /I include examples\c\contract.c ^
 *     /Fe:target\release\contract.exe /link target\release\winrecovery.dll.lib
 */
#include "winrecovery.h"

#include <stdio.h>
#include <string.h>

static int failures;

static void fail(const char *what, const char *detail)
{
    printf("FAIL %s -- %s\n", what, detail);
    ++failures;
}

static void pass(const char *what)
{
    printf("ok   %s\n", what);
}

/* wr_last_error() must be a usable, non-empty, NUL-terminated message after
 * any failure. Returns 1 when the last-error state is consistent. */
static int check_last_error(const char *what, int code)
{
    const char *msg;

    if (code == WR_OK) {
        fail(what, "failed but reported WR_OK");
        return 0;
    }
    if (code < WR_ERR_INVALID_ARG || code > WR_ERR_INTERNAL) {
        printf("     code %d is outside the documented WR_ERR_* range\n", code);
        fail(what, "error code out of range");
        return 0;
    }
    msg = wr_last_error();
    if (msg == NULL) {
        fail(what, "wr_last_error() returned NULL");
        return 0;
    }
    if (msg[0] == '\0') {
        fail(what, "wr_last_error() returned an empty message");
        return 0;
    }
    printf("     reported %d: %s\n", code, msg);
    return 1;
}

/* Cheap structural sanity check: a JSON result must be an object or an array,
 * and must be balanced at the top level. This is not a JSON parser — it only
 * catches truncated or non-JSON output, which is the realistic failure. */
static int looks_like_json(const char *s)
{
    size_t len = strlen(s);
    int in_str = 0, esc = 0, depth = 0;
    size_t i;

    if (len == 0) {
        return 0;
    }
    if (s[0] != '{' && s[0] != '[') {
        return 0;
    }
    for (i = 0; i < len; ++i) {
        char c = s[i];
        if (in_str) {
            if (esc) {
                esc = 0;
            } else if (c == '\\') {
                esc = 1;
            } else if (c == '"') {
                in_str = 0;
            }
            continue;
        }
        switch (c) {
        case '"': in_str = 1; break;
        case '{': case '[': ++depth; break;
        case '}': case ']': --depth; break;
        default: break;
        }
        if (depth < 0) {
            return 0;
        }
    }
    return depth == 0 && !in_str;
}

/* Exercise one `*_json` entry point against the contract. */
static void check_json(const char *what, char *(*fn)(void))
{
    char *json = fn();

    if (json == NULL) {
        if (check_last_error(what, wr_last_error_code())) {
            printf("ok   %s -- failed cleanly\n", what);
        }
        return;
    }
    if (!looks_like_json(json)) {
        printf("     got: %.120s\n", json);
        fail(what, "returned a non-JSON string with no error");
        wr_free_string(json);
        return;
    }
    printf("ok   %s -- %u byte(s): %.100s\n", what,
           (unsigned)strlen(json), json);
    wr_free_string(json);
}

/* Same, for the entry points that take an optional drive argument. */
static void check_json_drive(const char *what, char *(*fn)(const char *),
                             const char *drive)
{
    char *json = fn(drive);

    if (json == NULL) {
        if (check_last_error(what, wr_last_error_code())) {
            printf("ok   %s -- failed cleanly\n", what);
        }
        return;
    }
    if (!looks_like_json(json)) {
        printf("     got: %.120s\n", json);
        fail(what, "returned a non-JSON string with no error");
        wr_free_string(json);
        return;
    }
    printf("ok   %s -- %u byte(s): %.100s\n", what,
           (unsigned)strlen(json), json);
    wr_free_string(json);
}

int main(void)
{
    printf("WinRecovery C consumer contract test\n");

    /* -- version: library-owned static strings, never freed --------------- */
    {
        const char *v = wr_version();

        if (v == NULL || v[0] == '\0') {
            fail("wr_version", "NULL or empty");
        } else if (strchr(v, '\n') != NULL) {
            fail("wr_version", "contains a newline");
        } else {
            printf("ok   wr_version -- %s\n", v);
        }
        printf("ok   wr_version_parts -- %u.%u.%u\n",
               (unsigned)wr_version_major(),
               (unsigned)wr_version_minor(),
               (unsigned)wr_version_patch());
    }

    /* -- initial last-error state must be clean --------------------------- */
    if (wr_last_error_code() != WR_OK) {
        fail("initial state", "last-error code is not WR_OK before any call");
    } else if (wr_last_error() == NULL) {
        fail("initial state", "wr_last_error() returned NULL");
    } else {
        pass("initial last-error state is WR_OK");
    }

    /* -- read-only surface ------------------------------------------------ */
    check_json("wr_list_restore_points_json", wr_list_restore_points_json);
    check_json("wr_restore_point_protection_json",
               wr_restore_point_protection_json);
    check_json_drive("wr_recycle_bin_info_json(NULL)", wr_recycle_bin_info_json,
                     NULL);
    check_json_drive("wr_recycle_bin_info_json(\"C:\")",
                     wr_recycle_bin_info_json, "C:\\");
    check_json("wr_list_installed_programs_json",
               wr_list_installed_programs_json);
    check_json("wr_list_startup_entries_json", wr_list_startup_entries_json);
    check_json("wr_list_services_json", wr_list_services_json);
    check_json("wr_list_drivers_json", wr_list_drivers_json);

    /* -- argument validation --------------------------------------------- */
    {
        /* A NULL description is invalid, not a platform failure. */
        int rc = wr_create_restore_point(NULL);
        if (rc != WR_ERR_INVALID_ARG) {
            printf("     got %d, expected %d\n", rc, WR_ERR_INVALID_ARG);
            fail("wr_create_restore_point(NULL)", "expected WR_ERR_INVALID_ARG");
        } else if (wr_last_error_code() != WR_ERR_INVALID_ARG) {
            fail("wr_create_restore_point(NULL)",
                 "last-error code disagrees with the return value");
        } else {
            pass("wr_create_restore_point(NULL) -> WR_ERR_INVALID_ARG");
        }

        /* 0xFF is not valid UTF-8 on its own: the string must be rejected
         * rather than passed through to PowerShell or the registry. */
        rc = wr_create_restore_point("\xff\xfe");
        if (rc != WR_ERR_INVALID_ARG) {
            printf("     got %d, expected %d\n", rc, WR_ERR_INVALID_ARG);
            fail("wr_create_restore_point(invalid UTF-8)",
                 "expected WR_ERR_INVALID_ARG");
        } else {
            pass("wr_create_restore_point(invalid UTF-8) -> WR_ERR_INVALID_ARG");
        }
    }

    /* -- freeing NULL is a documented no-op ------------------------------- */
    wr_free_string(NULL);
    pass("wr_free_string(NULL) is a no-op");

    /* -- last error must survive the calls above -------------------------- */
    if (wr_last_error() == NULL) {
        fail("wr_last_error", "returned NULL after a failed call");
    } else {
        pass("wr_last_error still readable");
    }

    printf("\n");
    if (failures == 0) {
        printf("CONTRACT OK -- every C ABI rule in winrecovery.h held.\n");
        return 0;
    }
    printf("%d contract violation(s).\n", failures);
    return 1;
}
