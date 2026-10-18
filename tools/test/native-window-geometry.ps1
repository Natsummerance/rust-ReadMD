param([int]$OwnerProcessId)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ReadMDWindowGeometry {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
 [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo { public int cbSize; public Rect monitor, work; public uint flags; }
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
 [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);
 [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
}
'@
[void][ReadMDWindowGeometry]::SetProcessDPIAware()
$ownedProcess=Get-Process -Id $OwnerProcessId
$ownedHandle=$ownedProcess.MainWindowHandle
if($ownedHandle -eq [IntPtr]::Zero){throw 'Owned test window not ready'}
$ownedRect=New-Object ReadMDWindowGeometry+Rect
if(![ReadMDWindowGeometry]::GetWindowRect($ownedHandle,[ref]$ownedRect)){throw 'GetWindowRect failed'}
$ownedMonitor=New-Object ReadMDWindowGeometry+MonitorInfo
$ownedMonitor.cbSize=[Runtime.InteropServices.Marshal]::SizeOf($ownedMonitor)
if(![ReadMDWindowGeometry]::GetMonitorInfo([ReadMDWindowGeometry]::MonitorFromWindow($ownedHandle,2),[ref]$ownedMonitor)){throw 'GetMonitorInfo failed'}
@{window=$ownedRect;monitor=$ownedMonitor.monitor} | ConvertTo-Json -Compress
