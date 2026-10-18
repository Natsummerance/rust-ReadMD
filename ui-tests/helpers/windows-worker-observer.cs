// Windows-only QA barrier for genuine owned workers; never a replacement engine.
// Native contracts: Microsoft Learn SuspendThread, ResumeThread, CreateFileW,
// GetFinalPathNameByHandleW. One ResumeThread balances our one suspension.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;

public sealed class ReadMDWorkerObserver : IDisposable {
    const uint Timeout = 258, Failed = 0xffffffff;
    static readonly IntPtr Invalid = new IntPtr(-1);
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    struct ProcessRow { public uint size, usage, pid; public UIntPtr heap; public uint module, threads, parent; public int priority; public uint flags; [MarshalAs(UnmanagedType.ByValTStr, SizeConst=260)] public string exe; }
    [StructLayout(LayoutKind.Sequential)]
    struct ThreadRow { public uint size, usage, tid, owner; public int priority, delta; public uint flags; }
    [StructLayout(LayoutKind.Sequential)]
    struct FileInfo { public uint attributes; public System.Runtime.InteropServices.ComTypes.FILETIME created, accessed, modified; public uint volume, high, low, links, indexHigh, indexLow; }
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint pid);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool Process32FirstW(IntPtr snapshot, ref ProcessRow row);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool Process32NextW(IntPtr snapshot, ref ProcessRow row);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool Thread32First(IntPtr snapshot, ref ThreadRow row);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool Thread32Next(IntPtr snapshot, ref ThreadRow row);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenThread(uint access, bool inherit, uint tid);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint GetProcessIdOfThread(IntPtr thread);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder name, ref uint size);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint SuspendThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateProcess(IntPtr process, uint code);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern uint GetFinalPathNameByHandleW(IntPtr handle, StringBuilder name, uint size, uint flags);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetFileInformationByHandle(IntPtr handle, out FileInfo info);

    sealed class State {
        public IntPtr owner, process;
        public uint ownerPid, pid;
        public string program, kind, sha;
        public readonly Dictionary<uint,IntPtr> threads = new Dictionary<uint,IntPtr>();
    }
    readonly string root;
    readonly uint nodePid;
    bool validated, disposed;
    readonly Dictionary<string,State> states = new Dictionary<string,State>();
    static void Require(bool condition, string message) { if(!condition) throw new InvalidOperationException(message); }
    static Win32Exception NativeError(string message) { return new Win32Exception(Marshal.GetLastWin32Error(),message); }
    static bool Same(string a, string b) { return String.Equals(a,b,StringComparison.OrdinalIgnoreCase); }
    static string Plain(string name) { return name.StartsWith(@"\\?\UNC\",StringComparison.OrdinalIgnoreCase) ? @"\\"+name.Substring(8) : name.StartsWith(@"\\?\",StringComparison.Ordinal) ? name.Substring(4) : name; }
    static string Extended(string name) { return name.StartsWith(@"\\?\",StringComparison.Ordinal) ? name : name.StartsWith(@"\\",StringComparison.Ordinal) ? @"\\?\UNC\"+name.Substring(2) : @"\\?\"+name; }
    static string Parent(string name) { int slash=name.LastIndexOf('\\'); Require(slash>2,"Missing owned parent"); return name.Substring(0,slash); }
    static string Canonical(string name) {
        IntPtr file=CreateFileW(Extended(name),0,7,IntPtr.Zero,3,0x02000000,IntPtr.Zero);
        if(file==IntPtr.Zero||file==Invalid) throw NativeError("Cannot resolve verified path");
        try { var buffer=new StringBuilder(32768); uint size=GetFinalPathNameByHandleW(file,buffer,32768,0); if(size==0) throw NativeError("Cannot resolve final path"); Require(size<32768,"Final path exceeds native bound"); return Plain(buffer.ToString()).TrimEnd('\\'); }
        finally { CloseHandle(file); }
    }
    static string Image(IntPtr process) { var buffer=new StringBuilder(32768); uint size=32768; if(!QueryFullProcessImageNameW(process,0,buffer,ref size)) throw NativeError("Cannot identify owned process"); return Canonical(buffer.ToString()); }
    static string Digest(string file) { using(var stream=new FileStream(Extended(file),FileMode.Open,FileAccess.Read,FileShare.Read)) using(var hash=SHA256.Create()) return BitConverter.ToString(hash.ComputeHash(stream)).Replace("-","").ToLowerInvariant(); }
    static bool Alive(IntPtr handle) { Require(handle!=IntPtr.Zero,"Missing pinned process handle"); uint result=WaitForSingleObject(handle,0); if(result==Failed) throw NativeError("Cannot wait on pinned process"); Require(result==0||result==Timeout,"Unexpected process wait"); return result==Timeout; }
    static Dictionary<string,object> Result(params object[] pairs) { var result=new Dictionary<string,object>(); for(int i=0;i<pairs.Length;i+=2) result.Add((string)pairs[i],pairs[i+1]); return result; }
    static Dictionary<uint,uint> Rows(bool process) {
        IntPtr snapshot=CreateToolhelp32Snapshot(process?2u:4u,0); if(snapshot==IntPtr.Zero||snapshot==Invalid) throw NativeError("Native snapshot failed"); var result=new Dictionary<uint,uint>();
        try {
            bool active;
            if(process) { var row=new ProcessRow(); row.size=(uint)Marshal.SizeOf(typeof(ProcessRow)); active=Process32FirstW(snapshot,ref row); while(active){result[row.pid]=row.parent;row.size=(uint)Marshal.SizeOf(typeof(ProcessRow));active=Process32NextW(snapshot,ref row);} }
            else { var row=new ThreadRow(); row.size=(uint)Marshal.SizeOf(typeof(ThreadRow));active=Thread32First(snapshot,ref row);while(active){result[row.tid]=row.owner;row.size=(uint)Marshal.SizeOf(typeof(ThreadRow));active=Thread32Next(snapshot,ref row);} }
            Require(Marshal.GetLastWin32Error()==18,"Native snapshot did not finish normally"); return result;
        } finally { CloseHandle(snapshot); }
    }
    void Owned(string path) { Require(path.StartsWith(root+"\\",StringComparison.OrdinalIgnoreCase),"Engine/slot must be inside owned test profile"); }
    public ReadMDWorkerObserver(string qaRoot, uint testNodePid, string qaBase) {
        root=Canonical(qaRoot);nodePid=testNodePid;Require(Same(Parent(root),Canonical(qaBase))&&root.Substring(root.LastIndexOf('\\')+1).StartsWith("engine-mcp-native-",StringComparison.Ordinal),"Observer root is not a dedicated owned QA profile");
    }
    public object Ready(string echo) {
        var parents=Rows(true); uint current=(uint)Process.GetCurrentProcess().Id;
        for(int i=0;i<8;i++){if(!parents.TryGetValue(current,out current))break;if(current==nodePid){Require(Rows(false).ContainsValue((uint)Process.GetCurrentProcess().Id),"Native thread snapshot cannot see observer");validated=true;return Result("nodeAncestor",true,"echo",echo);}}
        throw new InvalidOperationException("Declared test Node process is not an observer ancestor");
    }
    public object Arm(uint ownerPid, string program, string ownerProgram, string kind, string sha) {
        Require(validated,"Observer ancestry must be checked first");Require(kind=="typst"||kind=="ocr","Unknown engine kind");string expected=Canonical(program);Owned(expected);Require(Same(Digest(expected),sha),"Engine bytes changed");uint parent;Require(Rows(true).TryGetValue(ownerPid,out parent)&&parent==nodePid,"Owner is not this test child");
        IntPtr owner=OpenProcess(0x101001,false,ownerPid);if(owner==IntPtr.Zero)throw NativeError("Cannot observe owner");
        try{Require(Same(Image(owner),Canonical(ownerProgram)),"Owner image mismatch");Require(Alive(owner),"Owner has already exited");Require(Rows(true).TryGetValue(ownerPid,out parent)&&parent==nodePid,"Owner ancestry changed");string key=Guid.NewGuid().ToString();states.Add(key,new State{owner=owner,ownerPid=ownerPid,program=expected,kind=kind,sha=sha});owner=IntPtr.Zero;return Result("key",key,"armed",true);}
        finally{if(owner!=IntPtr.Zero)CloseHandle(owner);}
    }
    static void ReleaseThreads(State state) {
        foreach(var thread in state.threads.Values)try{if(state.process!=IntPtr.Zero&&Alive(state.process))ResumeThread(thread);}finally{CloseHandle(thread);}state.threads.Clear();
    }
    static void ClearProcess(State state) { ReleaseThreads(state);if(state.process!=IntPtr.Zero){CloseHandle(state.process);state.process=IntPtr.Zero;} }
    bool Freeze(State state, uint pid) {
        Require(state.process==IntPtr.Zero,"This observation already holds a worker");IntPtr process=OpenProcess(0x101001,false,pid);if(process==IntPtr.Zero)return false;
        try{
            if(!Alive(state.owner)||!Alive(process))return false;
            string actual;try{actual=Image(process);}catch(Win32Exception){if(!Alive(process))return false;throw;}
            if(!Same(actual,state.program)||!Alive(process))return false;
            uint parent;bool owned=Rows(true).TryGetValue(pid,out parent)&&parent==state.ownerPid;
            // A real short --version process can finish between its image
            // query and the second ancestry snapshot. Only its signalled
            // pinned handle permits skipping it; a live mismatch fails closed.
            if(!owned&&!Alive(process))return false;Require(owned,"Engine parent changed");state.process=process;state.pid=pid;process=IntPtr.Zero;int lastError=0,currentCount=0;
            for(int attempt=0;attempt<5;attempt++){
                if(!Alive(state.process)){ClearProcess(state);return false;}
                foreach(var row in Rows(false)){
                    if(row.Value!=pid||state.threads.ContainsKey(row.Key))continue;IntPtr thread=OpenThread(0x802,false,row.Key);if(thread==IntPtr.Zero){lastError=Marshal.GetLastWin32Error();continue;}
                    bool retained=false;try{if(GetProcessIdOfThread(thread)!=pid){lastError=Marshal.GetLastWin32Error();continue;}if(SuspendThread(thread)==Failed){lastError=Marshal.GetLastWin32Error();continue;}state.threads.Add(row.Key,thread);retained=true;}finally{if(!retained)CloseHandle(thread);}
                }
                bool complete=true;currentCount=0;foreach(var row in Rows(false))if(row.Value==pid){currentCount++;if(!state.threads.ContainsKey(row.Key))complete=false;}
                if(complete&&currentCount>0){Require(Alive(state.process),"Worker exited during observation");return true;}Thread.Sleep(10);
            }
            if(!Alive(state.process)){ClearProcess(state);return false;}
            throw new InvalidOperationException("Live worker thread set did not stabilize: observed="+state.threads.Count+" current="+currentCount+" winerror="+lastError);
        }catch{ClearProcess(state);throw;}finally{if(process!=IntPtr.Zero)CloseHandle(process);}
    }
    static bool Arguments(State state) {
        // Query exactly the verified frozen process, never other applications.
        // A separate owned query process preserves an eight-second hard bound,
        // including a stalled WMI service or initial connection. Enumeration
        // timeouts alone would not bound the initial query/connection.
        Require(Alive(state.process),"Worker exited before command verification");string command;
        string code="[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false);$p=Get-CimInstance -ClassName Win32_Process -Filter \"ProcessId = "+state.pid+"\";if($null -eq $p){exit 2};[Console]::Write($p.CommandLine)";
        var info=new ProcessStartInfo(Path.Combine(Environment.GetEnvironmentVariable("SystemRoot"),@"System32\WindowsPowerShell\v1.0\powershell.exe"),"-NoProfile -NonInteractive -EncodedCommand "+Convert.ToBase64String(Encoding.Unicode.GetBytes(code))){UseShellExecute=false,CreateNoWindow=true,RedirectStandardOutput=true,RedirectStandardError=true,StandardOutputEncoding=new UTF8Encoding(false),StandardErrorEncoding=new UTF8Encoding(false)};
        var clock=Stopwatch.StartNew();using(var child=Process.Start(info)){
            IntPtr ownedQuery=child.Handle;var output=child.StandardOutput.ReadToEndAsync();var errors=child.StandardError.ReadToEndAsync();
            try{Require(child.WaitForExit(Math.Max(0,8000-(int)clock.ElapsedMilliseconds)),"Owned command query deadline");Require(Task.WaitAll(new Task[]{output,errors},Math.Max(0,8000-(int)clock.ElapsedMilliseconds)),"Owned command query pipe deadline");Require(child.ExitCode==0,"Cannot verify actual worker command");command=output.Result;}
            finally{if(Alive(ownedQuery)){if(!TerminateProcess(ownedQuery,1))throw NativeError("Cannot stop owned command query");Require(WaitForSingleObject(ownedQuery,2000)==0,"Owned query cleanup deadline");}}
        }
        Require(command!=null&&command.Replace(@"\\?\","").IndexOf(state.program,StringComparison.OrdinalIgnoreCase)>=0,"Worker command image mismatch");Require(Alive(state.process),"Worker exited during command verification");return state.kind=="typst"?Regex.IsMatch(command,@"\bcompile\b"):command.Contains("--tessdata-dir")&&Regex.IsMatch(command,@"\bstdin\b");
    }
    State Find(string key) { State state;Require(states.TryGetValue(key,out state),"Unknown owned observation");return state; }
    public object Observe(string key) {
        State state=Find(key);var clock=Stopwatch.StartNew();int skipped=0;
        while(clock.ElapsedMilliseconds<70000){Require(Alive(state.owner),"Actual owner exited before engine execution");foreach(var row in Rows(true)){
            if(row.Value!=state.ownerPid||!Freeze(state,row.Key))continue;
            if(Arguments(state)){string actual=Digest(state.program);Require(Same(actual,state.sha),"Observed engine bytes changed");return Result("key",key,"pid",state.pid,"threads",state.threads.Count,"genuineEngineSha256",actual,"compileOrRecognition",true,"physicalSuspension",true,"versionProcessesResumed",skipped);}
            ReleaseThreads(state);IntPtr version=state.process;state.process=IntPtr.Zero;try{Require(WaitForSingleObject(version,2000)==0,"Resumed version probe did not exit");}finally{CloseHandle(version);}skipped++;
        }Thread.Sleep(5);}throw new InvalidOperationException("Actual compile/recognition observation deadline");
    }
    public object FreeSlots(string key) {
        State state=Find(key);string directory=state.program;for(int i=0;i<4;i++)directory=Parent(directory);directory=Canonical(directory+"\\.locks");Owned(directory);int free=0;
        for(int i=0;i<2;i++){IntPtr file=CreateFileW(Extended(directory+"\\"+state.kind+".worker-"+i+".lock"),0x80000000,7,IntPtr.Zero,3,0x200000,IntPtr.Zero);if(file==IntPtr.Zero||file==Invalid){if(Marshal.GetLastWin32Error()==32)continue;throw NativeError("Cannot observe owned slot");}try{FileInfo info;if(!GetFileInformationByHandle(file,out info))throw NativeError("Cannot inspect opened slot");Require((info.attributes&(0x400|0x10))==0&&info.high==0&&info.low==0,"Unexpected slot file");free++;}finally{CloseHandle(file);}}
        return Result("free",free,"total",2,"readOnlyObservation",true);
    }
    public object IsAlive(string key) { return Result("alive",Alive(Find(key).process)); }
    public object Stopped(string key, int timeoutMs) { var clock=Stopwatch.StartNew();uint result=WaitForSingleObject(Find(key).process,(uint)Math.Max(0,Math.Min(timeoutMs,2000)));if(result==Failed)throw NativeError("Cannot wait for owned worker");return Result("stopped",result==0,"waitMs",clock.ElapsedMilliseconds); }
    public void Dispose() {
        if(disposed)return;disposed=true;var handles=new List<IntPtr>();foreach(var state in states.Values){if(state.owner!=IntPtr.Zero)handles.Add(state.owner);if(state.process!=IntPtr.Zero)handles.Add(state.process);}var clock=Stopwatch.StartNew();
        try{
            foreach(var handle in handles)if(Alive(handle))WaitForSingleObject(handle,(uint)Math.Max(0,2000-clock.ElapsedMilliseconds));
            foreach(var state in states.Values){if(state.owner!=IntPtr.Zero&&Alive(state.owner)){Console.Error.WriteLine("{\"rescue\":\"owned-owner\",\"key\":"+state.pid+"}");if(!TerminateProcess(state.owner,1))throw NativeError("Owned owner rescue failed");}if(state.process!=IntPtr.Zero&&Alive(state.process)){Console.Error.WriteLine("{\"rescue\":\"owned-process\",\"key\":"+state.pid+"}");if(!TerminateProcess(state.process,1))throw NativeError("Owned worker rescue failed");}}
            clock.Restart();foreach(var handle in handles)if(Alive(handle))WaitForSingleObject(handle,(uint)Math.Max(0,2000-clock.ElapsedMilliseconds));
        }finally{foreach(var state in states.Values){ReleaseThreads(state);if(state.owner!=IntPtr.Zero)CloseHandle(state.owner);if(state.process!=IntPtr.Zero)CloseHandle(state.process);}states.Clear();}
    }
}
