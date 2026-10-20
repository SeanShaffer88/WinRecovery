/* WinRecovery C example — build (Windows, MSVC):
 *   cl /I ..\..\include example.c /link ..\..\target\release\winrecovery.dll.lib
 * or link the DLL import library produced by your Rust build.
 */
#include "winrecovery.h"
#include <stdio.h>

int main(void) {
    printf("WinRecovery %s\n", wr_version());

    char *programs = wr_list_installed_programs_json();
    if (!programs) {
        fprintf(stderr, "programs failed (%d): %s\n",
                wr_last_error_code(), wr_last_error());
        return 1;
    }
    printf("Installed programs JSON: %.120s...\n", programs);
    wr_free_string(programs);

    char *bin = wr_recycle_bin_info_json(NULL);
    if (bin) {
        printf("Recycle Bin: %s\n", bin);
        wr_free_string(bin);
    } else {
        fprintf(stderr, "recycle bin failed (%d): %s\n",
                wr_last_error_code(), wr_last_error());
    }
    return 0;
}
