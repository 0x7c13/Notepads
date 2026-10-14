// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Threading;

// External OS contracts only. The installed Production executable has no test hooks.
public sealed class PackagedAppDriver : IDisposable
{
    [ComImport, Guid("2E941141-7F97-4756-BA1D-9DECDE894A3D"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IActivation
    {
        void ActivateApplication([MarshalAs(UnmanagedType.LPWStr)] string id,
            [MarshalAs(UnmanagedType.LPWStr)] string args, uint options, out uint pid);
        void ActivateForFile([MarshalAs(UnmanagedType.LPWStr)] string id, IntPtr items,
            [MarshalAs(UnmanagedType.LPWStr)] string verb, out uint pid);
        void ActivateForProtocol([MarshalAs(UnmanagedType.LPWStr)] string id, IntPtr items, out uint pid);
    }

    // Vtable order from shobjidl_core.h; unused methods retain their slots.
    [ComImport, Guid("F27C3930-8029-4AD1-94E3-3DBA417810C1"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IPackageDebug
    {
        void EnableDebugging([MarshalAs(UnmanagedType.LPWStr)] string package, IntPtr debugger, IntPtr environment);
        void DisableDebugging([MarshalAs(UnmanagedType.LPWStr)] string package);
        void Suspend([MarshalAs(UnmanagedType.LPWStr)] string package);
        void Resume([MarshalAs(UnmanagedType.LPWStr)] string package);
        void TerminateAllProcesses([MarshalAs(UnmanagedType.LPWStr)] string package);
        void SetTargetSessionId(uint session);
        void EnumerateBackgroundTasks(IntPtr package, out uint count, out IntPtr ids, out IntPtr names);
        void ActivateBackgroundTask(IntPtr task);
        void StartServicing(IntPtr package);
        void StopServicing(IntPtr package);
        void StartSessionRedirection(IntPtr package, uint session);
        void StopSessionRedirection(IntPtr package);
        void GetPackageExecutionState([MarshalAs(UnmanagedType.LPWStr)] string package, out int state);
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    private static extern void SHCreateItemFromParsingName(string path, IntPtr context, ref Guid iid, out IntPtr item);
    [DllImport("shell32.dll", PreserveSig = false)]
    private static extern void SHCreateShellItemArrayFromShellItem(IntPtr item, ref Guid iid, out IntPtr array);
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr root, EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, Input[] input, int size);
    [StructLayout(LayoutKind.Explicit, Size = 40)]
    private struct Input
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public ushort Key;
        [FieldOffset(10)] public ushort Scan;
        [FieldOffset(12)] public uint Flags;
    }

    private readonly IActivation activation = (IActivation)Activator.CreateInstance(
        System.Type.GetTypeFromCLSID(new Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")));
    private readonly IPackageDebug lifecycle = (IPackageDebug)Activator.CreateInstance(
        System.Type.GetTypeFromCLSID(new Guid("B1AEC16F-2383-4852-B0E9-8F0B1DC66B4D")));
    private readonly string appId, package;

    public PackagedAppDriver(string appId, string package)
    {
        if (IntPtr.Size != 8) throw new PlatformNotSupportedException("The desktop driver requires a 64-bit process.");
        this.appId = appId;
        this.package = package;
    }

    public uint Launch()
    {
        activation.ActivateApplication(appId, "", 0, out uint pid);
        return pid;
    }

    public uint OpenFile(string path)
    {
        Guid itemId = new Guid("43826D1E-E718-42EE-BC55-A1E261C37BFE");
        Guid arrayId = new Guid("B63EA76D-1F85-456F-A19C-48159EFA858B");
        SHCreateItemFromParsingName(path, IntPtr.Zero, ref itemId, out IntPtr item);
        IntPtr array = IntPtr.Zero;
        try
        {
            SHCreateShellItemArrayFromShellItem(item, ref arrayId, out array);
            activation.ActivateForFile(appId, array, "open", out uint pid);
            return pid;
        }
        finally
        {
            if (array != IntPtr.Zero) Marshal.Release(array);
            Marshal.Release(item);
        }
    }

    public void Suspend() => lifecycle.Suspend(package);
    public void Resume() => lifecycle.Resume(package);
    public void Terminate() => lifecycle.TerminateAllProcesses(package);
    public int State { get { lifecycle.GetPackageExecutionState(package, out int state); return state; } }

    private static bool ContainsProcess(IntPtr root, uint pid)
    {
        if (root == IntPtr.Zero || !IsWindowVisible(root)) return false;
        GetWindowThreadProcessId(root, out uint rootPid);
        if (rootPid == pid) return true;
        bool found = false;
        EnumChildWindows(root, (child, _) =>
        {
            GetWindowThreadProcessId(child, out uint childPid);
            found = childPid == pid && IsWindowVisible(child);
            return !found;
        }, IntPtr.Zero);
        return found;
    }

    public static IntPtr Window(uint pid)
    {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, _) =>
        {
            if (!ContainsProcess(window, pid)) return true;
            result = window;
            return false;
        }, IntPtr.Zero);
        return result;
    }

    public static void Focus(uint pid)
    {
        IntPtr window = Window(pid);
        if (window == IntPtr.Zero) throw new InvalidOperationException("No visible window for owned process " + pid);
        ShowWindowAsync(window, 9);
        SetForegroundWindow(window);
        for (int i = 0; i < 30; i++)
        {
            if (ContainsProcess(GetForegroundWindow(), pid)) return;
            Thread.Sleep(50);
        }
        throw new InvalidOperationException("Could not foreground owned process " + pid);
    }

    private static void RequireFocus(uint pid)
    {
        if (!ContainsProcess(GetForegroundWindow(), pid))
            throw new InvalidOperationException("Foreground changed; refusing keyboard input.");
    }

    private static void Send(ushort key, ushort scan, uint flags)
    {
        var input = new[] { new Input { Type = 1, Key = key, Scan = scan, Flags = flags } };
        if (SendInput(1, input, 40) != 1) throw new Win32Exception(Marshal.GetLastWin32Error(), "SendInput failed.");
    }

    public static void Key(uint pid, ushort key, bool control, bool shift)
    {
        RequireFocus(pid);
        bool ctrlDown = false, shiftDown = false, keyDown = false;
        try
        {
            if (control) { Send(0x11, 0, 0); ctrlDown = true; }
            if (shift) { Send(0x10, 0, 0); shiftDown = true; }
            Send(key, 0, 0); keyDown = true;
        }
        finally
        {
            if (keyDown) Send(key, 0, 2);
            if (shiftDown) Send(0x10, 0, 2);
            if (ctrlDown) Send(0x11, 0, 2);
        }
    }

    public static void Type(uint pid, string text)
    {
        foreach (char c in text)
        {
            RequireFocus(pid);
            Send(0, c, 4);
            Send(0, c, 6);
        }
    }

    public static void Close(uint pid)
    {
        IntPtr window = Window(pid);
        if (window == IntPtr.Zero || !PostMessage(window, 0x10, IntPtr.Zero, IntPtr.Zero))
            throw new InvalidOperationException("Cannot request normal close of owned window.");
    }

    public void Dispose()
    {
        Marshal.FinalReleaseComObject(activation);
        Marshal.FinalReleaseComObject(lifecycle);
    }
}
