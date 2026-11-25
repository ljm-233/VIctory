param([Parameter(Mandatory=$true)][int]$GameProcessId, [int]$TimeoutSeconds=60)
$ErrorActionPreference='Stop'
if (-not ('Victory.ClientArea' -as [type])) {
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace Victory {
 public static class ClientArea {
  [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] struct MonitorInfo { public int Size; public Rect Desktop,Work; public int Flags; }
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h,out Rect rect);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out Rect rect);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint id);
  [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr h,uint flags);
  [DllImport("user32.dll")] static extern bool GetMonitorInfoW(IntPtr m,ref MonitorInfo info);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h,IntPtr z,int x,int y,int w,int height,uint flags);
  [DllImport("user32.dll")] static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] static extern bool ShowWindowAsync(IntPtr h,int cmd);
  public static string Set1080(IntPtr h, int pid) {
    uint owner; GetWindowThreadProcessId(h,out owner);
    if(owner!=(uint)pid || h==IntPtr.Zero) return null;
    IntPtr previous=SetThreadDpiAwarenessContext(new IntPtr(-4));
    try {
      if(IsZoomed(h)) {ShowWindowAsync(h,9);return null;}
      Rect c,w;
      if(!GetClientRect(h,out c)||!GetWindowRect(h,out w))return null;
      if(c.Right==1920 && c.Bottom==1080)return "1920x1080";
      int width=1920+w.Right-w.Left-c.Right, height=1080+w.Bottom-w.Top-c.Bottom;
      var mon=new MonitorInfo();mon.Size=Marshal.SizeOf(typeof(MonitorInfo));
      int x=w.Left,y=w.Top;
      if(GetMonitorInfoW(MonitorFromWindow(h,2),ref mon)){
        x=mon.Work.Left+Math.Max(0,(mon.Work.Right-mon.Work.Left-width)/2);
        y=mon.Work.Top+Math.Max(0,(mon.Work.Bottom-mon.Work.Top-height)/2);
      }
      SetWindowPos(h,IntPtr.Zero,x,y,width,height,0x4014);
      return null;
    } finally {if(previous!=IntPtr.Zero) SetThreadDpiAwarenessContext(previous);}
  }
 }
}
"@
}
$timer=[Diagnostics.Stopwatch]::StartNew()
while ($timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
    $game=Get-Process -Id $GameProcessId -ErrorAction SilentlyContinue
    if (-not $game) { throw "Game exited before its window could be resized." }
    if ($game.MainWindowTitle -match 'Americas|VIctory') {
        $size=[Victory.ClientArea]::Set1080($game.MainWindowHandle,$GameProcessId)
        if($size){Write-Output "Game client area: $size physical pixels (PID $GameProcessId)"; return}
    }
    Start-Sleep -Milliseconds 250
}
throw "No 1920x1080 game window confirmed within $TimeoutSeconds seconds (PID $GameProcessId)."
