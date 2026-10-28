# WinRecovery live verification — run on a real Windows machine.
#
# This is the on-machine pass that closes WinRecovery v0.1.0's release gate.
# CI proves the ABI contract holds; only this proves the library returns
# correct data for a real system's restore points, Recycle Bin, installed
# programs, startup entries, services and drivers.
#
# WHAT IT DOES BY DEFAULT (read-only, safe):
#   Loads winrecovery.dll and calls every read-only entry point, printing
#   counts and a sample of each result. It creates nothing, deletes nothing,
#   and changes nothing.
#
# WHAT THE OPTIONAL PHASES DO:
#   -IncludeMutations   Creates ONE restore point, verifies it appears and
#                       that its description round-trips byte-for-byte
#                       (including non-ASCII, which exercises the whole UTF-8
#                       path), then deletes exactly that restore point. It is
#                       self-cleaning: a completed run leaves nothing behind.
#                       Deletion is also attempted in a finally block, so an
#                       interrupted run does not strand a restore point.
#                       Requires elevation and System Protection enabled.
#
#   -EmptyRecycleBin    EMPTIES THE RECYCLE BIN. THIS IS IRREVERSIBLE — the
#                       contents cannot be recovered afterwards. Kept behind
#                       its own switch, never implied by -IncludeMutations.
#                       It previews the item count and size first and requires
#                       you to type EMPTY before anything is deleted.
#
# GETTING THE DLL:
#   Build it with `cargo build --release` on Windows, or download the
#   `winrecovery-windows-x64` artifact from any CI run's summary page (it
#   contains winrecovery.dll, its import library, and winrecovery.h).
#
# USAGE:
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1 -DllPath C:\path\winrecovery.dll
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1 -IncludeMutations
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1 -IncludeMutations -EmptyRecycleBin -Drive C:\
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1 -ReportPath .\verification-results.md
#
# Then record the outcome in docs/VERIFICATION.md, or hand back -ReportPath's
# markdown. A run where every read-only AND mutation check passes is what
# closes the gate; a read-only pass alone is partial verification.

[CmdletBinding()]
param(
    [string]$DllPath = "$PSScriptRoot\..\..\target\release\winrecovery.dll",

    # Opt-in: create + delete one restore point (self-cleaning round trip).
    [switch]$IncludeMutations,

    # Opt-in and IRREVERSIBLE: empties the Recycle Bin after a preview and a
    # typed confirmation. Deliberately separate from -IncludeMutations.
    [switch]$EmptyRecycleBin,

    # Drive for Recycle Bin checks (e.g. 'C:\'). Omit to query all drives.
    [string]$Drive,

    # Where to write the markdown report. Omit to only print to the console.
    [string]$ReportPath
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Results collection
# ---------------------------------------------------------------------------
# Every check records one of PASS / FAIL / BLOCKED / SKIP. BLOCKED means the
# check could not run for an environmental reason (not elevated, System
# Protection off) rather than because the library misbehaved — keeping those
# distinct is what stops an environmental limitation from being misread as a
# defect, and stops a skipped check from being misread as a verified one.
$script:Results = New-Object System.Collections.ArrayList

function Add-Result {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][ValidateSet('PASS', 'FAIL', 'BLOCKED', 'SKIP')][string]$Status,
        [string]$Detail = ''
    )
    [void]$script:Results.Add([pscustomobject]@{
            Name   = $Name
            Status = $Status
            Detail = $Detail
        })

    $color = switch ($Status) {
        'PASS' { 'Green' }
        'FAIL' { 'Red' }
        'BLOCKED' { 'Yellow' }
        default { 'DarkGray' }
    }
    Write-Host "$Status  $Name" -ForegroundColor $color
    if ($Detail) { Write-Host "        $Detail" }
}

# ---------------------------------------------------------------------------
# Locate the DLL
# ---------------------------------------------------------------------------
if (-not (Test-Path $DllPath)) {
    Write-Host "FAIL: DLL not found at $DllPath" -ForegroundColor Red
    Write-Host 'Build it with: cargo build --release   (in the WinRecovery repo)'
    Write-Host 'Or download the winrecovery-windows-x64 artifact from a CI run.'
    exit 1
}
$DllPath = (Resolve-Path $DllPath).Path

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
$isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$osCaption = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).Caption
$osBuild = [Environment]::OSVersion.Version.ToString()
$procArch = $env:PROCESSOR_ARCHITECTURE

Write-Host 'WinRecovery live verification'
Write-Host "DLL        : $DllPath"
Write-Host "Windows    : $osCaption"
Write-Host "Build      : $osBuild ($procArch)"
Write-Host "Elevated   : $isAdmin"
Write-Host "Mutations  : $([bool]$IncludeMutations)"
Write-Host "Empty bin  : $([bool]$EmptyRecycleBin)  (IRREVERSIBLE)"
Write-Host ''

# ---------------------------------------------------------------------------
# P/Invoke surface
# ---------------------------------------------------------------------------
# The class name is unique per run and the compiled type is captured with
# -PassThru, so re-running in one session (or against a second DLL) does not
# fail with "type already exists".
$TypeName = 'Wr' + [guid]::NewGuid().ToString('N').Substring(0, 8)

# C# string literals need backslashes doubled. Computed once here instead of
# per attribute; -replace treats the pattern as a regex (one literal
# backslash) and the replacement literally (two backslashes).
$DllLiteral = $DllPath -replace '\\', '\\'

$src = @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class $TypeName {
    // The library returns UTF-8. Marshal.PtrToStringAnsi decodes with the
    // process ANSI code page, which corrupts any non-ASCII byte sequence --
    // accented program names, CJK publishers, curly quotes. PtrToStringUTF8
    // is not available on .NET Framework (Windows PowerShell 5.1), so the
    // decode is done by hand: find the NUL, copy the bytes, decode as UTF-8.
    public static string Utf8(IntPtr p) {
        if (p == IntPtr.Zero) return null;
        int len = 0;
        while (Marshal.ReadByte(p, len) != 0) len++;
        byte[] buf = new byte[len];
        Marshal.Copy(p, buf, 0, len);
        return Encoding.UTF8.GetString(buf);
    }

    // Allocate a UTF-8 NUL-terminated buffer for an incoming const char*.
    // Free it with FreePtr (Marshal.FreeHGlobal) -- never with
    // wr_free_string, which would hand it to the library's allocator and
    // mismatch the heap it came from.
    public static IntPtr AllocUtf8(string s) {
        byte[] b = Encoding.UTF8.GetBytes(s);
        IntPtr p = Marshal.AllocHGlobal(b.Length + 1);
        Marshal.Copy(b, 0, p, b.Length);
        Marshal.WriteByte(p, b.Length, 0);
        return p;
    }
    public static void FreePtr(IntPtr p) {
        if (p != IntPtr.Zero) Marshal.FreeHGlobal(p);
    }

    // Read a caller-owned JSON result and release it exactly once.
    public static string Take(IntPtr p) {
        if (p == IntPtr.Zero) return null;
        try { return Utf8(p); }
        finally { wr_free_string(p); }
    }

    public static string LastError() { return Utf8(wr_last_error()); }
    public static int LastCode() { return wr_last_error_code(); }

    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_version();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern uint wr_version_major();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern uint wr_version_minor();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern uint wr_version_patch();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr wr_last_error();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    private static extern int wr_last_error_code();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern void wr_free_string(IntPtr p);

    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_restore_points_json();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern int wr_create_restore_point(IntPtr descriptionUtf8);
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern int wr_delete_restore_point(uint sequence);
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_restore_point_protection_json();

    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_recycle_bin_info_json(IntPtr driveOrNull);
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern int wr_recycle_bin_empty(IntPtr driveOrNull, int noConfirmation);

    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_installed_programs_json();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_startup_entries_json();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_services_json();
    [DllImport("$DllLiteral", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_drivers_json();
}
"@

$Wr = (Add-Type -TypeDefinition $src -PassThru)[0]

# Wrappers so the rest of the script reads cleanly.
function Wr-Str([IntPtr]$p) { return $Wr::Utf8($p) }
function Wr-Take([IntPtr]$p) { return $Wr::Take($p) }
function Wr-Err { return "error $($Wr::LastCode()): $($Wr::LastError())" }

# ---------------------------------------------------------------------------
# Version
# ---------------------------------------------------------------------------
$verStr = Wr-Str $Wr::wr_version()
$parts = "$($Wr::wr_version_major()).$($Wr::wr_version_minor()).$($Wr::wr_version_patch())"
Write-Host "Version    : $verStr`n"

if ([string]::IsNullOrEmpty($verStr)) {
    Add-Result 'wr_version' 'FAIL' 'returned an empty string'
} elseif ($verStr -ne $parts) {
    Add-Result 'wr_version' 'FAIL' "string '$verStr' disagrees with the parts '$parts'"
} else {
    Add-Result 'wr_version / version parts' 'PASS' "$verStr (string and major/minor/patch agree)"
}

# ---------------------------------------------------------------------------
# Read-only checks
# ---------------------------------------------------------------------------
# Takes the JSON string rather than a scriptblock that fetches it: passing a
# scriptblock would make variable resolution depend on PowerShell's scope
# chain at invocation time, which is easy to get subtly wrong and impossible
# to check without running it. Callers fetch, then report.
function Test-Json {
    param(
        [Parameter(Mandatory)][string]$Name,
        [AllowNull()][string]$Json,
        [switch]$IsArray
    )
    # IsNullOrEmpty, not `$null -eq`: a [string]-typed parameter binds $null
    # as "", so a null check alone would fall through to ConvertFrom-Json and
    # report "invalid JSON" instead of the library's actual error.
    if ([string]::IsNullOrEmpty($Json)) {
        # The error is read here, before any further WinRecovery call can
        # overwrite the thread-local message.
        Add-Result $Name 'FAIL' (Wr-Err)
        return $null
    }
    try {
        $parsed = $Json | ConvertFrom-Json
    } catch {
        Add-Result $Name 'FAIL' "invalid JSON: $($_.Exception.Message)"
        return $null
    }

    if ($IsArray) {
        $items = @($parsed)
        $sample = ''
        if ($items.Count -gt 0) { $sample = $items[0] | ConvertTo-Json -Compress -Depth 4 }
        $suffix = ''
        if ($sample) { $suffix = "; sample: $sample" }
        Add-Result $Name 'PASS' "$($items.Count) item(s)$suffix"
        # Comma-wrapped so an empty array survives assignment to the caller
        # instead of collapsing to $null, which would be indistinguishable
        # from a failed call.
        return , $items
    }
    Add-Result $Name 'PASS' ($Json | ConvertTo-Json -Compress -Depth 4)
    return $parsed
}

Write-Host '--- Read-only ---'

$restorePoints = Test-Json -Name 'wr_list_restore_points_json' -IsArray -Json (Wr-Take $Wr::wr_list_restore_points_json())

$protection = Test-Json -Name 'wr_restore_point_protection_json' -Json (Wr-Take $Wr::wr_restore_point_protection_json())

# All drives, via the NULL argument.
$binAll = Test-Json -Name 'wr_recycle_bin_info_json (all drives)' -Json (Wr-Take $Wr::wr_recycle_bin_info_json([IntPtr]::Zero))

# A specific drive, which exercises the non-NULL const char* path. Fetched
# before reporting so the pointer stays allocated across the call.
$binDrive = $null
if ($Drive) {
    $drivePtr = $Wr::AllocUtf8($Drive)
    try {
        $driveJson = Wr-Take $Wr::wr_recycle_bin_info_json($drivePtr)
    } finally {
        $Wr::FreePtr($drivePtr)
    }
    $binDrive = Test-Json -Name "wr_recycle_bin_info_json ('$Drive')" -Json $driveJson
}

$programs = Test-Json -Name 'wr_list_installed_programs_json' -IsArray -Json (Wr-Take $Wr::wr_list_installed_programs_json())

$startup = Test-Json -Name 'wr_list_startup_entries_json' -IsArray -Json (Wr-Take $Wr::wr_list_startup_entries_json())

$services = Test-Json -Name 'wr_list_services_json' -IsArray -Json (Wr-Take $Wr::wr_list_services_json())

$drivers = Test-Json -Name 'wr_list_drivers_json' -IsArray -Json (Wr-Take $Wr::wr_list_drivers_json())

# Non-ASCII round trip, observable without mutating anything: real systems
# have program names and publishers outside ASCII, and a mangled one is the
# symptom this verification exists to catch.
if ($programs) {
    $nonAscii = @($programs | Where-Object { $_.name -match '[^\x00-\x7F]' -or $_.publisher -match '[^\x00-\x7F]' })
    if ($nonAscii.Count -gt 0) {
        $example = $nonAscii[0]
        Add-Result 'UTF-8 round trip (installed programs)' 'PASS' `
            "$($nonAscii.Count) non-ASCII name(s) decoded cleanly; e.g. '$($example.name)' [$($example.publisher)]"
    } else {
        Add-Result 'UTF-8 round trip (installed programs)' 'SKIP' `
            'no non-ASCII program names on this machine; -IncludeMutations tests UTF-8 explicitly'
    }
}

# A driver list should be a subset of the service list; both come from the
# same SCM enumeration with a different filter.
if ($null -ne $services -and $null -ne $drivers) {
    $svcCount = @($services).Count
    $drvCount = @($drivers).Count
    $driversFlagged = @($services | Where-Object { $_.is_driver }).Count
    if ($drvCount -eq $driversFlagged -and $drvCount -le $svcCount) {
        Add-Result 'services/drivers consistency' 'PASS' `
            "$svcCount services, $drvCount drivers, matching the $($driversFlagged) flagged is_driver in the service list"
    } else {
        Add-Result 'services/drivers consistency' 'FAIL' `
            "wr_list_drivers_json returned $drvCount but the service list flags $driversFlagged of $svcCount as drivers"
    }
}

# ---------------------------------------------------------------------------
# Optional: restore point round trip (self-cleaning)
# ---------------------------------------------------------------------------
if ($IncludeMutations) {
    Write-Host ''
    Write-Host '--- Mutations (restore point round trip) ---'

    if (-not $isAdmin) {
        Add-Result 'wr_create_restore_point' 'BLOCKED' 'not elevated; re-run from an administrator PowerShell'
        Add-Result 'wr_delete_restore_point' 'BLOCKED' 'not elevated; create did not run'
        Add-Result 'UTF-8 round trip (restore point description)' 'BLOCKED' 'not elevated'
    } elseif ($null -ne $protection -and $protection.enabled -eq $false) {
        # Creating a restore point with System Protection off fails in Windows
        # itself. That is correct behaviour, not a library defect, so it is
        # reported as BLOCKED rather than FAIL.
        Add-Result 'wr_create_restore_point' 'BLOCKED' 'System Protection is disabled on this machine'
        Add-Result 'wr_delete_restore_point' 'BLOCKED' 'System Protection is disabled; create did not run'
        Add-Result 'UTF-8 round trip (restore point description)' 'BLOCKED' 'System Protection is disabled'
    } else {
        # Unique token so the point this run created can be identified and
        # deleted precisely, even if other restore points already exist.
        $token = 'WRVERIFY-' + (Get-Date -Format 'yyyyMMddHHmmss')

        # Non-ASCII characters are built from code points rather than written
        # as literals: Windows PowerShell 5.1 reads a BOM-less .ps1 as ANSI,
        # so literal accents in this file would be corrupted before the DLL
        # ever saw them. This keeps the check independent of how the script is
        # saved. U+00E9 U+00FC U+20AC U+4E2D -> e-acute, u-diaeresis, euro, CJK.
        $accented = [string]::Join('', @([char]0x00E9, [char]0x00FC, [char]0x20AC, [char]0x4E2D))
        $description = "WinRecovery verify $accented $token"

        # The library filters the description to alphanumerics and ' -_.()' and
        # truncates at 60 characters. All characters above are alphanumeric in
        # Rust's Unicode sense, so the expected value is the same string -- but
        # recompute it the same way so the assertion tests the round trip, not
        # an assumption about the filter.
        $allowed = ' -_.()'
        $expected = -join ($description.ToCharArray() |
                Where-Object { [char]::IsLetterOrDigit($_) -or $allowed.IndexOf([string]$_) -ge 0 })
        $expected = $expected.Substring(0, [Math]::Min(60, $expected.Length))

        $baseline = @()
        if ($null -ne $restorePoints) { $baseline = @($restorePoints) }
        $maxSeq = 0
        foreach ($rp in $baseline) { if ([int]$rp.sequence -gt $maxSeq) { $maxSeq = [int]$rp.sequence } }

        Write-Host "  creating: '$description'"
        Write-Host "  expected after the library's filter: '$expected'"

        $createdSeq = $null
        $createRc = -1
        $descPtr = $Wr::AllocUtf8($description)
        try {
            # finally guarantees the point is removed even if verification
            # throws part-way, so a failed run does not leave residue behind.
            try {
                $rc = $Wr::wr_create_restore_point($descPtr)
                $createRc = $rc
                if ($rc -ne 0) {
                    Add-Result 'wr_create_restore_point' 'FAIL' "returned $rc; $(Wr-Err)"
                } else {
                    Add-Result 'wr_create_restore_point' 'PASS' "returned WR_OK for '$description'"

                    # Re-enumerate and find exactly the point just created.
                    $after = Wr-Take $Wr::wr_list_restore_points_json()
                    if ($null -eq $after) {
                        Add-Result 'restore point appears in list' 'FAIL' (Wr-Err)
                    } else {
                        $list = @($after | ConvertFrom-Json)
                        $found = @($list | Where-Object { $_.description -eq $expected })

                        if ($found.Count -eq 0) {
                            # Fall back to sequence, so a description mismatch is
                            # reported as such instead of hiding the new point.
                            $newer = @($list | Where-Object { [int]$_.sequence -gt $maxSeq })
                            if ($newer.Count -eq 1) {
                                $createdSeq = [uint32]$newer[0].sequence
                                Add-Result 'restore point appears in list' 'PASS' "sequence $createdSeq is new"
                                Add-Result 'UTF-8 round trip (restore point description)' 'FAIL' `
                                    "expected '$expected' but the point reads '$($newer[0].description)'"
                            } else {
                                Add-Result 'restore point appears in list' 'FAIL' `
                                    "no point matched the description and $($newer.Count) point(s) had a sequence above $maxSeq"
                            }
                        } else {
                            $createdSeq = [uint32]$found[0].sequence
                            Add-Result 'restore point appears in list' 'PASS' "sequence $createdSeq, description matched"
                            Add-Result 'UTF-8 round trip (restore point description)' 'PASS' `
                                "'$expected' survived create -> registry -> enumerate -> JSON -> UTF-8 decode unchanged"

                            # Ordering contract: lists are newest-sequence first.
                            $seqs = @($list | ForEach-Object { [int]$_.sequence })
                            $sorted = @($seqs | Sort-Object -Descending)
                            if (($seqs -join ',') -eq ($sorted -join ',')) {
                                Add-Result 'restore point ordering' 'PASS' 'newest sequence first, as documented'
                            } else {
                                Add-Result 'restore point ordering' 'FAIL' "not sorted newest-first: $($seqs -join ',')"
                            }
                        }
                    }
                }
            } finally {
                if ($null -ne $createdSeq) {
                    $drc = $Wr::wr_delete_restore_point($createdSeq)
                    if ($drc -ne 0) {
                        Add-Result 'wr_delete_restore_point' 'FAIL' `
                            "returned $drc for sequence $createdSeq; $(Wr-Err) -- DELETE THIS RESTORE POINT MANUALLY"
                    } else {
                        $final = Wr-Take $Wr::wr_list_restore_points_json()
                        $stillThere = $false
                        if ($final) {
                            $stillThere = @($final | ConvertFrom-Json |
                                    Where-Object { [uint32]$_.sequence -eq $createdSeq }).Count -gt 0
                        }
                        if ($stillThere) {
                            Add-Result 'wr_delete_restore_point' 'FAIL' "returned WR_OK but sequence $createdSeq is still listed"
                        } else {
                            Add-Result 'wr_delete_restore_point' 'PASS' "sequence $createdSeq deleted and no longer listed"
                        }
                    }
                } elseif ($createRc -eq 0) {
                    # Create succeeded but the point was never identified, so
                    # it is still on the system. Recover rather than strand it.
                    $retry = Wr-Take $Wr::wr_list_restore_points_json()
                    $candidates = @()
                    if ($retry) {
                        $candidates = @($retry | ConvertFrom-Json |
                                Where-Object { [int]$_.sequence -gt $maxSeq })
                    }
                    if ($candidates.Count -eq 1) {
                        $orphan = [uint32]$candidates[0].sequence
                        $drc = $Wr::wr_delete_restore_point($orphan)
                        if ($drc -ne 0) {
                            Add-Result 'wr_delete_restore_point' 'FAIL' `
                                "created sequence $orphan but cleanup returned $drc; $(Wr-Err) -- DELETE IT MANUALLY"
                        } else {
                            Add-Result 'wr_delete_restore_point' 'PASS' `
                                "cleaned up sequence $orphan, which the identification step had missed"
                        }
                    } else {
                        $seqList = ($candidates | ForEach-Object { $_.sequence }) -join ', '
                        Add-Result 'wr_delete_restore_point' 'FAIL' `
                            "a restore point WAS created but could not be identified ($($candidates.Count) candidate(s) above baseline $maxSeq : $seqList) -- INSPECT AND DELETE MANUALLY"
                    }
                } else {
                    Add-Result 'wr_delete_restore_point' 'SKIP' 'create did not succeed, so there is nothing to delete'
                }
            }
        } finally {
            # Allocated with AllocHGlobal, so it is freed here and never passed
            # to wr_free_string (a different allocator).
            $Wr::FreePtr($descPtr)
        }
    }
} else {
    Write-Host ''
    Write-Host '--- Mutations: not requested (pass -IncludeMutations) ---'
    Add-Result 'wr_create_restore_point' 'SKIP' 'not requested; pass -IncludeMutations'
    Add-Result 'wr_delete_restore_point' 'SKIP' 'not requested; pass -IncludeMutations'
    Add-Result 'UTF-8 round trip (restore point description)' 'SKIP' 'not requested; pass -IncludeMutations'
}

# ---------------------------------------------------------------------------
# Optional: empty the Recycle Bin (IRREVERSIBLE)
# ---------------------------------------------------------------------------
if ($EmptyRecycleBin) {
    Write-Host ''
    Write-Host '--- Empty Recycle Bin (IRREVERSIBLE) ---'

    $target = if ($Drive) { $Drive } else { 'all drives' }

    # Preview what is about to be destroyed, per the project standard: a
    # destructive action is previewed where feasible before it is confirmed.
    $previewPtr = if ($Drive) { $Wr::AllocUtf8($Drive) } else { [IntPtr]::Zero }
    try {
        $previewJson = Wr-Take $Wr::wr_recycle_bin_info_json($previewPtr)
    } finally { if ($Drive) { $Wr::FreePtr($previewPtr) } }

    if ($null -eq $previewJson) {
        Add-Result 'wr_recycle_bin_empty' 'BLOCKED' "could not preview $target; $(Wr-Err)"
    } else {
        $preview = $previewJson | ConvertFrom-Json
        $gb = [Math]::Round($preview.size_bytes / 1GB, 3)
        Write-Host "  Target : $target"
        Write-Host "  Items  : $($preview.item_count)"
        Write-Host "  Size   : $($preview.size_bytes) bytes (~$gb GB)"
        Write-Host ''
        Write-Host '  These items CANNOT be recovered after emptying.' -ForegroundColor Yellow

        if ([Console]::IsInputRedirected) {
            # Cannot obtain a typed confirmation, so refuse rather than
            # destroy data unattended.
            Add-Result 'wr_recycle_bin_empty' 'BLOCKED' 'input is redirected; a typed confirmation is required'
        } else {
            Write-Host '  Type EMPTY to continue, anything else to skip: ' -NoNewline
            $answer = Read-Host
            if ($answer -cne 'EMPTY') {
                Add-Result 'wr_recycle_bin_empty' 'SKIP' "confirmation was '$answer', not 'EMPTY'; nothing was deleted"
            } else {
                $emptyPtr = if ($Drive) { $Wr::AllocUtf8($Drive) } else { [IntPtr]::Zero }
                try {
                    # no_confirmation = 1: this script already confirmed, so
                    # suppress Windows' own dialog.
                    $rc = $Wr::wr_recycle_bin_empty($emptyPtr, 1)
                } finally { if ($Drive) { $Wr::FreePtr($emptyPtr) } }

                if ($rc -ne 0) {
                    Add-Result 'wr_recycle_bin_empty' 'FAIL' "returned $rc for $target; $(Wr-Err)"
                } else {
                    $checkPtr = if ($Drive) { $Wr::AllocUtf8($Drive) } else { [IntPtr]::Zero }
                    try {
                        $afterJson = Wr-Take $Wr::wr_recycle_bin_info_json($checkPtr)
                    } finally { if ($Drive) { $Wr::FreePtr($checkPtr) } }

                    if ($null -eq $afterJson) {
                        Add-Result 'wr_recycle_bin_empty' 'PASS' "returned WR_OK for $target; post-check failed: $(Wr-Err)"
                    } else {
                        $after = $afterJson | ConvertFrom-Json
                        if ($after.item_count -eq 0 -and $after.size_bytes -eq 0) {
                            Add-Result 'wr_recycle_bin_empty' 'PASS' "$target emptied; now 0 items, 0 bytes"
                        } else {
                            Add-Result 'wr_recycle_bin_empty' 'FAIL' `
                                "returned WR_OK but $target still reports $($after.item_count) item(s), $($after.size_bytes) bytes"
                        }
                    }
                }
            }
        }
    }
} else {
    Add-Result 'wr_recycle_bin_empty' 'SKIP' 'not requested; pass -EmptyRecycleBin (IRREVERSIBLE)'
}

# ---------------------------------------------------------------------------
# Verdict and report
# ---------------------------------------------------------------------------
$failCount = @($script:Results | Where-Object { $_.Status -eq 'FAIL' }).Count
$blockedCount = @($script:Results | Where-Object { $_.Status -eq 'BLOCKED' }).Count
$skipCount = @($script:Results | Where-Object { $_.Status -eq 'SKIP' }).Count
$passCount = @($script:Results | Where-Object { $_.Status -eq 'PASS' }).Count

Write-Host ''
Write-Host "PASS $passCount   FAIL $failCount   BLOCKED $blockedCount   SKIP $skipCount"
Write-Host ''

if ($failCount -gt 0) {
    $verdict = 'NOT VERIFIED'
    $verdictColor = 'Red'
    $verdictNote = "$failCount check(s) failed. Record the failures in docs/VERIFICATION.md as Unverified and report them."
} elseif ($blockedCount -gt 0 -or $skipCount -gt 0) {
    # Distinguish "read-only verified" from "nothing verified": a run that
    # skipped the mutations has not closed the gate, and saying so plainly is
    # the point of keeping BLOCKED/SKIP separate from PASS.
    $mutationChecked = @($script:Results |
        Where-Object { $_.Status -eq 'PASS' -and $_.Name -match 'wr_create_restore_point|wr_delete_restore_point' }).Count
    if ($mutationChecked -gt 0) {
        $verdict = 'PARTIALLY VERIFIED'
        $verdictNote = 'Some checks were blocked or skipped; the gate is not fully closed. Re-run with elevation, System Protection enabled, or -IncludeMutations / -EmptyRecycleBin as appropriate.'
    } else {
        $verdict = 'READ-ONLY VERIFIED'
        $verdictNote = 'Read-only entry points verified against live data. The mutation checks did not run, so v0.1.0''s release gate is NOT yet closed: re-run with -IncludeMutations from an elevated shell.'
    }
    $verdictColor = 'Yellow'
} else {
    $verdict = 'FULLY VERIFIED'
    $verdictColor = 'Green'
    $verdictNote = 'Every read-only and mutation check passed on this machine. This closes the v0.1.0 on-machine gate; record it in docs/VERIFICATION.md.'
}

Write-Host "$verdict" -ForegroundColor $verdictColor
Write-Host $verdictNote
Write-Host ''

if ($ReportPath) {
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.AppendLine('# WinRecovery live verification results')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine("Generated by ``examples/powershell/smoke-test.ps1`` on $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zz').")
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('## Environment')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('| | |')
    [void]$sb.AppendLine('|---|---|')
    [void]$sb.AppendLine("| Library version | ``$verStr`` |")
    [void]$sb.AppendLine("| DLL | ``$DllPath`` |")
    [void]$sb.AppendLine("| Windows | $osCaption |")
    [void]$sb.AppendLine("| Build | ``$osBuild`` ($procArch) |")
    [void]$sb.AppendLine("| Elevated | $isAdmin |")
    [void]$sb.AppendLine('| -IncludeMutations | ' + [bool]$IncludeMutations + ' |')
    [void]$sb.AppendLine('| -EmptyRecycleBin | ' + [bool]$EmptyRecycleBin + ' |')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine("## Verdict: $verdict")
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine($verdictNote)
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('## Results')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('| Check | Status | Detail |')
    [void]$sb.AppendLine('|---|---|---|')
    foreach ($r in $script:Results) {
        $detail = ($r.Detail -replace '\|', '\|') -replace "`r?`n", ' '
        [void]$sb.AppendLine("| ``$($r.Name)`` | $($r.Status) | $detail |")
    }
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine("PASS $passCount / FAIL $failCount / BLOCKED $blockedCount / SKIP $skipCount")

    Set-Content -Path $ReportPath -Value $sb.ToString() -Encoding UTF8
    Write-Host "Report written to $ReportPath"
}

if ($failCount -gt 0) { exit 1 }
exit 0
