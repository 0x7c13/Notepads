// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

// Process.Handle requests PROCESS_ALL_ACCESS, which a normal desktop process
// may not have for a broker-created UWP process. Retain only the rights needed
// by the isolated test runner, including after the application has exited.
public sealed class IsolatedTestProcess : IDisposable
{
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeWaitHandle OpenProcess(uint access, bool inherit, uint id);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetProcessTimes(SafeWaitHandle process, out long created, out long exited, out long kernel, out long user);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool QueryFullProcessImageNameW(SafeWaitHandle process, uint flags, StringBuilder name, ref int size);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(SafeWaitHandle process, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeProcess(SafeWaitHandle process, out int code);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateProcess(SafeWaitHandle process, int code);

    private readonly SafeWaitHandle handle;
    public uint Id { get; }
    public string Path { get; }
    public DateTime StartTime { get; }

    public IsolatedTestProcess(uint id)
    {
        const uint QueryLimited = 0x1000, Synchronize = 0x100000, Terminate = 1;
        handle = OpenProcess(QueryLimited | Synchronize | Terminate, false, id);
        try
        {
            if (handle.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot retain the test process.");
            Id = id;
            if (!GetProcessTimes(handle, out long created, out _, out _, out _)) throw new Win32Exception(Marshal.GetLastWin32Error());
            StartTime = DateTime.FromFileTimeUtc(created);
            var path = new StringBuilder(32768);
            int size = path.Capacity;
            if (!QueryFullProcessImageNameW(handle, 0, path, ref size)) throw new Win32Exception(Marshal.GetLastWin32Error());
            Path = path.ToString();
        }
        catch { handle.Dispose(); throw; }
    }

    public bool HasExited => WaitForExit(0);

    public bool WaitForExit(int milliseconds)
    {
        if (milliseconds < 0) throw new ArgumentOutOfRangeException(nameof(milliseconds));
        uint result = WaitForSingleObject(handle, (uint)milliseconds);
        if (result == 0) return true;
        if (result == 258) return false;
        throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot wait for the retained test process.");
    }

    public int ExitCode
    {
        get
        {
            if (!HasExited) throw new InvalidOperationException("The test process is still running.");
            if (!GetExitCodeProcess(handle, out int code)) throw new Win32Exception(Marshal.GetLastWin32Error());
            return code;
        }
    }

    public void Kill()
    {
        if (HasExited) return;
        if (!TerminateProcess(handle, -1))
        {
            int error = Marshal.GetLastWin32Error();
            if (!HasExited) throw new Win32Exception(error);
        }
    }

    public void Dispose() => handle.Dispose();
}
