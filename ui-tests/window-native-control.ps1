$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class ReadMDWindowFixture {
 delegate bool Callback(IntPtr h,IntPtr data);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr d);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] static extern IntPtr GetWindowLongPtrW(IntPtr h,int index);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
 [DllImport("user32.dll")] static extern bool IsZoomed(IntPtr h);
 [DllImport("user32.dll")] static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out Rect r);
 [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
 [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h,ref Point p);
 [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 public static IntPtr Find(uint pid,bool tray) {
  IntPtr found=IntPtr.Zero;
  EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);if(p!=pid)return true;
   var c=new StringBuilder(256);GetClassName(h,c,256);var t=new StringBuilder(256);GetWindowText(h,t,256);
   if(tray ? c.ToString()=="ReadMD.Tray.Owner" : t.ToString().Contains("ReadMD")&&!t.ToString().StartsWith("ReadMD · ")&&c.ToString()!="ReadMD.Tray.Owner") {found=h;return false;}return true;
  },IntPtr.Zero);return found;
 }
 public static string State(uint pid) {
  SetThreadDpiAwarenessContext(new IntPtr(-4)); var h=Find(pid,false);if(h==IntPtr.Zero)return "{}";
  Rect r;GetWindowRect(h,out r);long style=GetWindowLongPtrW(h,-16).ToInt64();
  var client=new Point();ClientToScreen(h,ref client);
  return "{\"visible\":"+IsWindowVisible(h).ToString().ToLower()+",\"minimized\":"+IsIconic(h).ToString().ToLower()+",\"maximized\":"+IsZoomed(h).ToString().ToLower()+",\"clientTop\":"+(client.Y-r.Top)+",\"dpi\":"+GetDpiForWindow(h)+",\"x\":"+r.Left+",\"y\":"+r.Top+",\"width\":"+(r.Right-r.Left)+",\"height\":"+(r.Bottom-r.Top)+"}";
 }
 public static bool Command(uint pid,string action) {
  bool tray=action=="tray-show";var h=Find(pid,tray);if(h==IntPtr.Zero)return false;
  return PostMessageW(h,tray?0x8033U:0x0010U,IntPtr.Zero,new IntPtr(tray?0x202:0));
 }
}
'@
if($request.action -eq 'state') { [ReadMDWindowFixture]::State([uint32]$request.pid) }
else { if(-not [ReadMDWindowFixture]::Command([uint32]$request.pid,[string]$request.action)) {throw 'Own fixture window was not found'}; '{"ok":true}' }
