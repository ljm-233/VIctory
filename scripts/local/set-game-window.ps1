param([Parameter(Mandatory=$true)][int]$GameProcessId, [int]$TimeoutSeconds=60)
$ErrorActionPreference='Stop'
if (-not ('Victory.ClientArea' -as [type])) {
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
namespace Victory {
 public static class ClientArea {
  [StructLayout(LayoutKind.Sequential)] struct Rect { public int Left,Top,Right,Bottom; }
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h,out Rect rect);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out Rect rect);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint id);
  [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr h,int attribute,out Rect rect,int size);
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
      // The outer Win32 rectangle includes an invisible resizing margin.
      Rect visible;
      if(DwmGetWindowAttribute(h,9,out visible,Marshal.SizeOf(typeof(Rect)))!=0) visible=w;
      if(c.Right==1920 && c.Bottom==1080 && visible.Left==0 && visible.Top==0)
        return "1920x1080, visible top-left (0,0)";
      int width=1920+w.Right-w.Left-c.Right, height=1080+w.Bottom-w.Top-c.Bottom;
      int x=w.Left-visible.Left, y=w.Top-visible.Top;
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
throw "Game window did not reach 1920x1080 at (0,0) within $TimeoutSeconds seconds (PID $GameProcessId)."
