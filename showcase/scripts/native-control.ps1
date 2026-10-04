$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
trap { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class ReadMDShowcaseWindow {
 public delegate bool Callback(IntPtr h, IntPtr data);
 [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb,IntPtr data);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int command);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int height,uint flags);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
 [DllImport("user32.dll")] public static extern void keybd_event(byte vk,byte scan,uint flags,UIntPtr extra);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr h,int index);
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
 [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent,Callback cb,IntPtr data);
 [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,string text);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
 [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a,uint b,bool attach);
 public static void Focus(IntPtr h) {
  uint p;uint fg=GetWindowThreadProcessId(GetForegroundWindow(),out p),own=GetCurrentThreadId();
  ShowWindow(h,9);
  AttachThreadInput(own,fg,true);SetForegroundWindow(h);AttachThreadInput(own,fg,false);
  if(GetForegroundWindow()!=h) {
   keybd_event(0x12,0,0,UIntPtr.Zero);keybd_event(0x12,0,2,UIntPtr.Zero);
   SetForegroundWindow(h);
  }
 }
 public static IntPtr Child(IntPtr h,int id,string klass) {
  IntPtr found=IntPtr.Zero;EnumChildWindows(h,(c,d)=>{var name=new StringBuilder(256);GetClassName(c,name,256);
   if(GetDlgCtrlID(c)==id&&name.ToString()==klass){found=c;return false;}return true;},IntPtr.Zero);return found;
 }
 public static void Confirm(IntPtr h) {
  IntPtr button=Child(h,1,"Button");
  if(button!=IntPtr.Zero)SendMessage(button,0x00f5,IntPtr.Zero,IntPtr.Zero);
  else SendMessage(h,0x0111,new IntPtr(1),IntPtr.Zero);
 }
 public static IntPtr FindPrint(uint[] owned) {
  IntPtr found=IntPtr.Zero;EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);if(Array.IndexOf(owned,p)<0||!IsWindowVisible(h))return true;
   var c=new StringBuilder(256);var t=new StringBuilder(512);GetClassName(h,c,256);GetWindowText(h,t,512);
   if(t.ToString().Contains("打印")||t.ToString().Contains("Print")||p!=owned[0]&&c.ToString().StartsWith("Chrome_WidgetWin")){found=h;return false;}return true;},IntPtr.Zero);return found;
 }
 public static uint ForegroundPid() {uint p;GetWindowThreadProcessId(GetForegroundWindow(),out p);return p;}
 public static void CloseDialogs(uint ownPid) {
  EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);var c=new StringBuilder(256);GetClassName(h,c,256);
   if(p==ownPid&&c.ToString()=="#32770")PostMessage(h,0x0010,IntPtr.Zero,IntPtr.Zero);return true;},IntPtr.Zero);
 }
 public static IntPtr Find(uint ownPid,bool dialog,bool aux,bool folder) {
  IntPtr found=IntPtr.Zero;
  EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);if(p!=ownPid)return true;
   var c=new StringBuilder(256);GetClassName(h,c,256);var t=new StringBuilder(512);GetWindowText(h,t,512);
   if(!IsWindowVisible(h)||!IsWindowEnabled(h))return true;
   if(dialog ? c.ToString()=="#32770" && (folder ? t.ToString().Contains("文件夹") : !t.ToString().Contains("文件夹")) : aux ? t.ToString().StartsWith("ReadMD · ") : t.ToString().Contains("ReadMD") && !t.ToString().StartsWith("ReadMD · ")){found=h;return false;}return true;
  },IntPtr.Zero);return found;
 }
}
'@
[void][ReadMDShowcaseWindow]::SetProcessDPIAware()
[void][ReadMDShowcaseWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
if($request.action -in @('autostart-save','autostart-restore')) {
 $backup=[IO.Path]::GetFullPath([string]$request.path)
 $allowed=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../.runtime'))+[IO.Path]::DirectorySeparatorChar
 if(-not $backup.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)){throw 'Startup backup leaves the isolated workspace'}
 $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Run',$true)
 if(-not $key){throw 'Current-user startup key is unavailable'}
 try {
  if($request.action -eq 'autostart-save') {
   $exists=$key.GetValueNames() -contains 'ReadMD';$value=$null;$kind='String'
   if($exists){$kind=$key.GetValueKind('ReadMD').ToString();if($kind -notin @('String','ExpandString')){throw 'Unexpected ReadMD startup value type'};$value=$key.GetValue('ReadMD',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)}
   @{exists=$exists;kind=$kind;value=$value} | ConvertTo-Json -Compress | Set-Content -LiteralPath $backup -Encoding UTF8
  } else {
   $saved=Get-Content -LiteralPath $backup -Raw | ConvertFrom-Json
   if($saved.exists){if($saved.kind -notin @('String','ExpandString')){throw 'Startup backup type is invalid'};$key.SetValue('ReadMD',[string]$saved.value,[Microsoft.Win32.RegistryValueKind]::$($saved.kind))}
   else{$key.DeleteValue('ReadMD',$false)}
  }
 } finally {$key.Close()}
 Write-Output '{"ok":true}'
 exit 0
}
if($request.action -eq 'dismiss-network') {
 $windows=[System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.OrCondition]::new([System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Windows 安全中心'),[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,'Windows Security Alert')))
 $allowed=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../.runtime')).Replace('/','\')
 foreach($candidate in $windows) {
  if($candidate.Current.Name -notmatch 'Windows.*(安全|Security)'){continue}
  $all=$candidate.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
  $names=@($all | ForEach-Object {$_.Current.Name}) -join "`n"
  if(-not $names.Contains($allowed)){continue}
  foreach($item in $all){if($item.Current.Name -in @('取消','Cancel')){([System.Windows.Automation.InvokePattern]$item.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke();break}}
 }
 Write-Output '{"ok":true}'
 exit 0
}
if ($request.action -in @('explorer-open','explorer-close')) {
 $folder=[IO.Path]::GetFullPath([string]$request.path)
 $allowed=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../.runtime'))+[IO.Path]::DirectorySeparatorChar
 if(-not $folder.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)){throw 'Explorer operation is outside isolated demonstration materials'}
 $shell=New-Object -ComObject Shell.Application
 if($request.action -eq 'explorer-open'){$shell.Open($folder)}
 $deadline=[DateTime]::UtcNow.AddSeconds(20);$own=$null
 do {
  foreach($candidate in $shell.Windows()) {try {if($candidate.Document.Folder.Self.Path -eq $folder){$own=$candidate;break}}catch{}}
  if($own -or $request.action -eq 'explorer-close'){break};Start-Sleep -Milliseconds 100
 }while([DateTime]::UtcNow -lt $deadline)
 if($request.action -eq 'explorer-close'){if($own){$own.Quit()};Write-Output '{"ok":true}';exit 0}
 if(-not $own){throw 'Own demonstration file manager was not found'}
 $handle=[IntPtr]$own.HWND
 [void][ReadMDShowcaseWindow]::SetWindowPos($handle,[IntPtr]::Zero,[int]$request.x,[int]$request.y,[int]$request.width,[int]$request.height,0x0040)
 [ReadMDShowcaseWindow]::Focus($handle)
 $leaf=[IO.Path]::GetFileName([string]$request.file);$item=$own.Document.Folder.ParseName($leaf)
 if(-not $item){throw 'Own demonstration file is missing'}
 $own.Document.SelectItem($item,29);Start-Sleep -Milliseconds 300
 $element=[System.Windows.Automation.AutomationElement]::FromHandle($handle)
 $names=[System.Windows.Automation.OrCondition]::new(
  [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$leaf),
  [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,[IO.Path]::GetFileNameWithoutExtension($leaf)))
 $selected=$element.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$names)
 if(-not $selected){throw 'Own demonstration file row was not found'}
 $rect=$selected.Current.BoundingRectangle
 if($rect.Width -le 0 -or $rect.Height -le 0){throw 'Own demonstration file row has no visible bounds'}
 Write-Output (@{ok=$true;source=@{x=[int]($rect.X+$rect.Width/2);y=[int]($rect.Y+$rect.Height/2)}} | ConvertTo-Json -Compress)
 exit 0
}
if($request.action -eq 'print-cancel') {
 $print=[ReadMDShowcaseWindow]::Find([uint32]$request.pid,$false,$false,$false)
 if($print -eq [IntPtr]::Zero){throw 'Own printing window was not found'}
 [ReadMDShowcaseWindow]::Focus($print)
 [ReadMDShowcaseWindow]::keybd_event(0x1b,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 80
 [ReadMDShowcaseWindow]::keybd_event(0x1b,0,2,[UIntPtr]::Zero)
 Write-Output '{"ok":true,"printDialog":true}'
 exit 0
}
if ($request.action -eq 'close-dialogs') {
 [ReadMDShowcaseWindow]::CloseDialogs([uint32]$request.pid)
 Write-Output '{"ok":true}'
 exit 0
}
$until = [DateTime]::UtcNow.AddSeconds(20)
$window = [IntPtr]::Zero
while ($window -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $until) {
 $window = [ReadMDShowcaseWindow]::Find([uint32]$request.pid, ($request.action -eq 'dialog'), ($request.action -eq 'resize-aux'), [bool]$request.folder)
 if ($window -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
}
if ($window -eq [IntPtr]::Zero) { throw 'Own demonstration window was not found' }
if ($request.action -eq 'close') {
 [void][ReadMDShowcaseWindow]::PostMessage($window,0x0010,[IntPtr]::Zero,[IntPtr]::Zero)
} elseif ($request.action -in @('place','bounds')) {
 if($request.action -eq 'place'){[void][ReadMDShowcaseWindow]::SetWindowPos($window,[IntPtr]::Zero,[int]$request.x,[int]$request.y,0,0,0x0045)}
 $rect=[ReadMDShowcaseWindow+Rect]::new();[void][ReadMDShowcaseWindow]::GetWindowRect($window,[ref]$rect)
 $flags=[ReadMDShowcaseWindow]::GetWindowLongPtrW($window,-20).ToInt64()
 Write-Output (@{ok=$true;transparent=($flags -band 0x20) -ne 0;topmost=($flags -band 0x8) -ne 0;petBounds=@{x=$rect.Left;y=$rect.Top;width=$rect.Right-$rect.Left;height=$rect.Bottom-$rect.Top}} | ConvertTo-Json -Compress)
 exit 0
} elseif ($request.action -eq 'focus') {
 [ReadMDShowcaseWindow]::Focus($window)
 Write-Output (@{ok=$true;foreground=([ReadMDShowcaseWindow]::ForegroundPid() -eq [uint32]$request.pid)} | ConvertTo-Json -Compress)
 exit 0
} elseif ($request.action -eq 'keys') {
 [ReadMDShowcaseWindow]::Focus($window)
 foreach($key in ([string]$request.keys).ToUpperInvariant().ToCharArray()) {
  [ReadMDShowcaseWindow]::keybd_event([byte][char]$key,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 90
  [ReadMDShowcaseWindow]::keybd_event([byte][char]$key,0,2,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 70
 }
} elseif ($request.action -eq 'mouse') {
 [void][ReadMDShowcaseWindow]::SetCursorPos([int]$request.x,[int]$request.y)
 Start-Sleep -Milliseconds 180
 $down=if($request.button -eq 'right'){8}else{2};$up=if($request.button -eq 'right'){16}else{4}
 if($request.click -or $request.drag){[ReadMDShowcaseWindow]::mouse_event($down,0,0,0,[UIntPtr]::Zero)}
 if($request.click){Start-Sleep -Milliseconds 80}
 if($request.drag){for($step=1;$step -le 12;$step++){[void][ReadMDShowcaseWindow]::SetCursorPos([int]($request.x+($request.toX-$request.x)*$step/12),[int]($request.y+($request.toY-$request.y)*$step/12));Start-Sleep -Milliseconds 35}}
 if($request.click -or $request.drag){[ReadMDShowcaseWindow]::mouse_event($up,0,0,0,[UIntPtr]::Zero)}
} elseif ($request.action -eq 'resize') {
 $ratio = [ReadMDShowcaseWindow]::GetDpiForWindow($window) / 96.0
 $captureWidth = [int]([Math]::Ceiling(1300 * $ratio / 2) * 2)
 $captureHeight = [int]([Math]::Ceiling(850 * $ratio / 2) * 2)
 [void][ReadMDShowcaseWindow]::SetWindowPos($window,[IntPtr]::Zero,40,40,$captureWidth,$captureHeight,0x0040)
 [ReadMDShowcaseWindow]::Focus($window)
 Write-Output (@{ok=$true; bounds=@{x=40;y=40;width=$captureWidth;height=$captureHeight}} | ConvertTo-Json -Compress)
 exit 0
} elseif ($request.action -eq 'resize-aux') {
 $ratio = [ReadMDShowcaseWindow]::GetDpiForWindow($window) / 96.0
 [void][ReadMDShowcaseWindow]::SetWindowPos($window,[IntPtr]::Zero,120,95,[int](1120*$ratio),[int](730*$ratio),0x0040)
} elseif ($request.action -eq 'dialog') {
 [ReadMDShowcaseWindow]::Focus($window)
 $element = [System.Windows.Automation.AutomationElement]::FromHandle($window)
 if ($request.folder) {
  # Navigate explicitly: an already-open library may differ from the document.
  [System.Windows.Forms.SendKeys]::SendWait('%d')
  Start-Sleep -Milliseconds 120
  [System.Windows.Forms.Clipboard]::SetText([string]$request.path)
  [System.Windows.Forms.SendKeys]::SendWait('^a^v{ENTER}')
  Start-Sleep -Milliseconds 350
  [ReadMDShowcaseWindow]::Confirm($window)
  Write-Output '{"ok":true}'
  exit 0
 } else {
  $field=[ReadMDShowcaseWindow]::Child($window,1001,'Edit')
  if ($field -ne [IntPtr]::Zero) {
   [void][ReadMDShowcaseWindow]::SendMessage($field,0x000c,[IntPtr]::Zero,[string]$request.path)
   [ReadMDShowcaseWindow]::Confirm($window)
   Write-Output '{"ok":true}'
   exit 0
  }
  # The standard Windows Save dialog exposes this access key across UIA providers.
  [System.Windows.Forms.SendKeys]::SendWait('%n')
  Start-Sleep -Milliseconds 100
  [System.Windows.Forms.Clipboard]::SetText([string]$request.path)
  [System.Windows.Forms.SendKeys]::SendWait('^a^v')
  Start-Sleep -Milliseconds 150
  [System.Windows.Forms.SendKeys]::SendWait('%s')
  Write-Output '{"ok":true}'
  exit 0
 }
}
Write-Output '{"ok":true}'
