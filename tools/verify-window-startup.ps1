param(
    [string]$Executable = "$PSScriptRoot/../target/release/ayther_play_ce.exe"
)

$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$outputDir = Join-Path $PSScriptRoot '../artifacts/window-startup'
[void](New-Item -ItemType Directory -Force -Path $outputDir)

# Exercise the real Windows window manager, including delayed library/cover updates.
# Only the instance started by this script is queried, restored and closed.
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class PlayStartupWindow {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr arg);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo {
        public int Size; public Rect Monitor, Work; public uint Flags;
    }
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr arg);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
}
'@

$started = [Diagnostics.Stopwatch]::StartNew()
$instance = Start-Process -FilePath $Executable -PassThru -WindowStyle Normal `
    -RedirectStandardOutput (Join-Path $outputDir 'stdout.log') `
    -RedirectStandardError (Join-Path $outputDir 'stderr.log')
$samples = [Collections.Generic.List[object]]::new()
$ownedWindows = [Collections.Generic.List[IntPtr]]::new()
$callback = [PlayStartupWindow+EnumProc]{ param($handle, $unused)
    $windowPid = [uint32]0
    [void][PlayStartupWindow]::GetWindowThreadProcessId($handle, [ref]$windowPid)
    if ($windowPid -eq $instance.Id -and [PlayStartupWindow]::IsWindowVisible($handle)) {
        $title = [Text.StringBuilder]::new(256)
        [void][PlayStartupWindow]::GetWindowText($handle, $title, $title.Capacity)
        if ($title.ToString() -eq 'Ayther Play') { $ownedWindows.Add($handle) }
    }
    return $true
}

try {
    foreach ($sampleAt in @(500, 1500, 6000, 10000)) {
        while ($started.ElapsedMilliseconds -lt $sampleAt) { Start-Sleep -Milliseconds 50 }
        $ownedWindows.Clear()
        [void][PlayStartupWindow]::EnumWindows($callback, [IntPtr]::Zero)
        foreach ($handle in $ownedWindows) {
            $rect = [PlayStartupWindow+Rect]::new()
            [void][PlayStartupWindow]::GetWindowRect($handle, [ref]$rect)
            $monitor = [PlayStartupWindow+MonitorInfo]::new()
            $monitor.Size = [Runtime.InteropServices.Marshal]::SizeOf($monitor)
            [void][PlayStartupWindow]::GetMonitorInfo(
                [PlayStartupWindow]::MonitorFromWindow($handle, 2), [ref]$monitor)
            $samples.Add([pscustomobject]@{
                Milliseconds = $started.ElapsedMilliseconds
                Maximized = [PlayStartupWindow]::IsZoomed($handle)
                Width = $rect.Right - $rect.Left
                Height = $rect.Bottom - $rect.Top
                WorkWidth = $monitor.Work.Right - $monitor.Work.Left
                WorkHeight = $monitor.Work.Bottom - $monitor.Work.Top
            })
        }
    }
    $samples | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputDir 'result.json')
    $samples | Format-Table -AutoSize
    if ($samples.Count -eq 0) { throw 'The test instance did not show a window.' }
    $settled = @($samples | Where-Object Milliseconds -ge 1000)
    if ($settled.Count -eq 0 -or @($settled | Where-Object {
        -not $_.Maximized -or [Math]::Abs($_.Width - $_.WorkWidth) -gt 32 -or
        [Math]::Abs($_.Height - $_.WorkHeight) -gt 32
    }).Count -gt 0) { throw 'The window did not remain maximized to the monitor work area.' }

    # Startup maximization must not prevent the user from restoring the window later.
    [void][PlayStartupWindow]::ShowWindow($ownedWindows[0], 9)
    Start-Sleep -Milliseconds 500
    if ([PlayStartupWindow]::IsZoomed($ownedWindows[0])) {
        throw 'The window could not be restored after startup.'
    }
    Write-Output 'PASS: starts maximized, remains maximized during loading, and can be restored.'
} finally {
    if (-not $instance.HasExited) {
        [void]$instance.CloseMainWindow()
        [void]$instance.WaitForExit(5000)
    }
}
