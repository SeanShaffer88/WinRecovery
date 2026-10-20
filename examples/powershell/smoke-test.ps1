# WinRecovery live smoke test — run on a real Windows machine.
#
# What it does: loads winrecovery.dll via P/Invoke and calls the READ-ONLY
# functions against your live system (restore points, protection status,
# Recycle Bin info, installed programs, startup entries, services, drivers),
# printing counts and a sample of each result. It creates nothing, deletes
# nothing, and changes nothing.
#
# What you need: winrecovery.dll — build it with `cargo build --release`
# in this repo on Windows, or take it from a CI run's artifacts. Put the
# DLL next to this script or pass its path:
#
#   powershell -ExecutionPolicy Bypass -File smoke-test.ps1 -DllPath C:\path\winrecovery.dll
#
# Then paste the output back to Muse (or note any FAIL lines) — a clean
# run closes WinRecovery v0.1.0's on-machine verification gate.

param([string]$DllPath = "$PSScriptRoot\..\..\target\release\winrecovery.dll")

$ErrorActionPreference = 'Stop'
if (-not (Test-Path $DllPath)) {
    Write-Host "FAIL: DLL not found at $DllPath" -ForegroundColor Red
    Write-Host "Build it first: cargo build --release  (in the WinRecovery repo)"
    exit 1
}
$DllPath = (Resolve-Path $DllPath).Path
Write-Host "WinRecovery smoke test"
Write-Host "DLL: $DllPath"
Write-Host "Windows: $([Environment]::OSVersion.VersionString)"
Write-Host ""

$src = @"
using System;
using System.Runtime.InteropServices;
public static class Wr {
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_version();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_restore_points_json();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_restore_point_protection_json();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_recycle_bin_info_json(IntPtr drive);
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_installed_programs_json();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_startup_entries_json();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_services_json();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_list_drivers_json();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern IntPtr wr_last_error();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern int wr_last_error_code();
    [DllImport("$($DllPath -replace '\\','\\')", CallingConvention = CallingConvention.Cdecl)]
    public static extern void wr_free_string(IntPtr p);
    public static string Take(IntPtr p) {
        if (p == IntPtr.Zero) return null;
        string s = Marshal.PtrToStringAnsi(p);
        wr_free_string(p);
        return s;
    }
    public static string LastError() {
        IntPtr p = wr_last_error();
        return p == IntPtr.Zero ? "" : Marshal.PtrToStringAnsi(p);
    }
}
"@
Add-Type -TypeDefinition $src

$fails = 0
function Report($name, $json, $isArray) {
    if ($null -eq $json) {
        Write-Host "FAIL $name — error $([Wr]::wr_last_error_code()): $([Wr]::LastError())" -ForegroundColor Red
        $script:fails++
        return
    }
    try {
        $parsed = $json | ConvertFrom-Json
        if ($isArray) {
            $count = @($parsed).Count
            Write-Host "PASS $name — $count item(s)" -ForegroundColor Green
            if ($count -gt 0) { Write-Host "     sample: $($parsed[0] | ConvertTo-Json -Compress)" }
        } else {
            Write-Host "PASS $name — $json" -ForegroundColor Green
        }
    } catch {
        Write-Host "FAIL $name — invalid JSON: $($_.Exception.Message)" -ForegroundColor Red
        $script:fails++
    }
}

$ver = [System.Runtime.InteropServices.Marshal]::PtrToStringAnsi([Wr]::wr_version())
Write-Host "Version: $ver`n"

Report "restore points"        ([Wr]::Take([Wr]::wr_list_restore_points_json())) $true
Report "protection status"     ([Wr]::Take([Wr]::wr_restore_point_protection_json())) $false
Report "recycle bin (all)"     ([Wr]::Take([Wr]::wr_recycle_bin_info_json([IntPtr]::Zero))) $false
Report "installed programs"    ([Wr]::Take([Wr]::wr_list_installed_programs_json())) $true
Report "startup entries"       ([Wr]::Take([Wr]::wr_list_startup_entries_json())) $true
Report "services"              ([Wr]::Take([Wr]::wr_list_services_json())) $true
Report "drivers"               ([Wr]::Take([Wr]::wr_list_drivers_json())) $true

Write-Host ""
if ($fails -eq 0) {
    Write-Host "ALL PASS — WinRecovery live verification complete." -ForegroundColor Green
    exit 0
} else {
    Write-Host "$fails check(s) failed — paste this output back for diagnosis." -ForegroundColor Red
    exit 1
}
