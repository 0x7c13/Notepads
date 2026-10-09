// Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE file in the project root for license information.

using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

// Loaded by Test-EditorDragDrop.ps1. Coordinates and SendInput are physical on
// both supported 64-bit architectures; only the two discovered roots are trusted.
public sealed class EditorDragInput : IDisposable
{
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; public Point(int x, int y) { X = x; Y = y; } }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] private struct MonitorInfo { public int Size; public Rect Monitor, Work; public uint Flags; }
    [StructLayout(LayoutKind.Explicit, Size = 40)] private struct Input
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public int X;
        [FieldOffset(12)] public int Y;
        [FieldOffset(16)] public uint MouseData;
        [FieldOffset(20)] public uint MouseFlags;
        [FieldOffset(24)] public uint MouseTime;
        [FieldOffset(32)] public UIntPtr MouseExtra;
        [FieldOffset(8)] public ushort VirtualKey;
        [FieldOffset(10)] public ushort Scan;
        [FieldOffset(12)] public uint KeyFlags;
        [FieldOffset(16)] public uint KeyTime;
        [FieldOffset(24)] public UIntPtr KeyExtra;
    }
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr parent, EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] private static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, Input[] inputs, int size);

    private readonly uint editorProcess, peerProcess;
    private readonly IntPtr priorDpi;
    private bool mouseDown, controlDown, escapeDown;
    private uint sourceProcess;
    public IntPtr EditorWindow { get; private set; }
    public IntPtr PeerWindow { get; private set; }
    public Rect WorkArea { get; private set; }
    public bool HasPressedInput { get { return mouseDown || controlDown || escapeDown; } }
    public bool HasMousePress { get { return mouseDown; } }
    public uint SourceProcessId { get { return sourceProcess; } }

    public EditorDragInput(uint editorPid, uint peerPid)
    {
        if (IntPtr.Size != 8 || Marshal.SizeOf(typeof(Input)) != 40) throw new InvalidOperationException("64-bit SendInput must use a 40-byte INPUT.");
        if (editorPid == peerPid || editorPid == 0 || peerPid == 0) throw new InvalidOperationException("Distinct owned process IDs are required.");
        editorProcess = editorPid; peerProcess = peerPid;
        priorDpi = SetThreadDpiAwarenessContext(new IntPtr(-4)); // PER_MONITOR_AWARE_V2
        if (priorDpi == IntPtr.Zero) throw new InvalidOperationException("Could not establish physical-coordinate DPI awareness.");
        try
        {
            EditorWindow = FindRoot(editorPid);
            PeerWindow = FindRoot(peerPid);
            AssertIdleInput();
        }
        catch { SetThreadDpiAwarenessContext(priorDpi); throw; }
    }

    private static uint ProcessOf(IntPtr window) { uint pid; GetWindowThreadProcessId(window, out pid); return pid; }
    private static bool ContainsProcess(IntPtr root, uint pid)
    {
        if (ProcessOf(root) == pid) return true;
        bool found = false;
        EnumChildWindows(root, delegate(IntPtr child, IntPtr ignored) { if (ProcessOf(child) == pid && IsWindowVisible(child)) found = true; return !found; }, IntPtr.Zero);
        return found;
    }
    private static IntPtr FindRoot(uint pid)
    {
        IntPtr found = IntPtr.Zero;
        EnumWindows(delegate(IntPtr window, IntPtr ignored)
        {
            Rect rect;
            if (IsWindowVisible(window) && GetWindowRect(window, out rect) && rect.Right - rect.Left > 150 && rect.Bottom - rect.Top > 100 && ContainsProcess(window, pid))
            {
                if (found != IntPtr.Zero) throw new InvalidOperationException("More than one visible root belongs to the test process.");
                found = window;
            }
            return true;
        }, IntPtr.Zero);
        if (found == IntPtr.Zero) throw new InvalidOperationException("Visible owned window not found for PID " + pid);
        return found;
    }
    public void ValidateWindows()
    {
        if (!IsWindow(EditorWindow) || !IsWindow(PeerWindow) || !ContainsProcess(EditorWindow, editorProcess) || !ContainsProcess(PeerWindow, peerProcess))
            throw new InvalidOperationException("An exact owned window/descendant relationship changed.");
    }
    private IntPtr RootFor(uint pid)
    {
        if (pid == editorProcess) return EditorWindow;
        if (pid == peerProcess) return PeerWindow;
        throw new InvalidOperationException("Process is not owned by this driver.");
    }
    public void Arrange()
    {
        ValidateWindows();
        var monitor = new MonitorInfo { Size = Marshal.SizeOf(typeof(MonitorInfo)) };
        if (!GetMonitorInfo(MonitorFromWindow(EditorWindow, 2), ref monitor)) throw new InvalidOperationException("Cannot read physical work area.");
        WorkArea = monitor.Work;
        int width = (monitor.Work.Right - monitor.Work.Left - 32) / 2;
        int height = monitor.Work.Bottom - monitor.Work.Top - 24;
        if (width < 380 || height < 360) throw new InvalidOperationException("Monitor work area is too small for two visible test windows.");
        ShowWindowAsync(EditorWindow, 9); ShowWindowAsync(PeerWindow, 9);
        if (!SetWindowPos(EditorWindow, IntPtr.Zero, monitor.Work.Left + 8, monitor.Work.Top + 8, width, height, 0x14) ||
            !SetWindowPos(PeerWindow, IntPtr.Zero, monitor.Work.Left + width + 24, monitor.Work.Top + 8, width, height, 0x14))
            throw new InvalidOperationException("Could not arrange the owned test windows.");
    }
    public void Activate(uint pid)
    {
        ValidateWindows();
        IntPtr root = RootFor(pid);
        SetForegroundWindow(root);
        DateTime deadline = DateTime.UtcNow.AddSeconds(2);
        while (GetAncestor(GetForegroundWindow(), 2) != root)
        {
            if (DateTime.UtcNow >= deadline) throw new InvalidOperationException("Could not activate the owned source window; no input was injected.");
            Thread.Sleep(15);
        }
        RequireForeground();
    }
    public void RequireForeground()
    {
        ValidateWindows();
        IntPtr root = GetAncestor(GetForegroundWindow(), 2);
        // Windows can briefly have no foreground window when OLE hands focus
        // back after a drop. Wait without injecting input; an unrelated window
        // still fails immediately.
        DateTime deadline = DateTime.UtcNow.AddMilliseconds(250);
        while (root == IntPtr.Zero && DateTime.UtcNow < deadline)
        {
            Thread.Sleep(10);
            ValidateWindows();
            root = GetAncestor(GetForegroundWindow(), 2);
        }
        if (root != EditorWindow && root != PeerWindow) throw new InvalidOperationException("Foreground moved outside the two owned windows: HWND " + root + ", PID " + ProcessOf(root));
    }
    public void RequirePoint(int x, int y, uint pid)
    {
        ValidateWindows();
        IntPtr hit = WindowFromPoint(new Point(x, y));
        if (GetAncestor(hit, 2) != RootFor(pid) || ProcessOf(hit) != pid)
            throw new InvalidOperationException("Input point does not hit the expected owned process: " + x + "," + y + " expected PID " + pid + " hit PID " + ProcessOf(hit));
    }
    public void AssertIdleInput()
    {
        foreach (int key in new int[] { 1, 2, 4, 0x10, 0x11, 0x12, 0x1B, 0x5B, 0x5C })
            if ((GetAsyncKeyState(key) & 0x8000) != 0) throw new InvalidOperationException("A physical mouse button or modifier is already pressed; refusing to take ownership of it.");
    }
    private static void Send(Input input)
    {
        if (SendInput(1, new Input[] { input }, 40) != 1) throw new InvalidOperationException("SendInput did not inject the requested event. Win32 error " + Marshal.GetLastWin32Error());
    }
    private static void Key(ushort key, bool up) { Send(new Input { Type = 1, VirtualKey = key, KeyFlags = up ? 2U : 0U }); }
    public void MoveTo(int x, int y, int steps)
    {
        Point from;
        if (!GetCursorPos(out from)) throw new InvalidOperationException("Cannot obtain physical cursor coordinates.");
        int left = GetSystemMetrics(76), top = GetSystemMetrics(77), width = GetSystemMetrics(78), height = GetSystemMetrics(79);
        if (width < 2 || height < 2 || x < left || y < top || x >= left + width || y >= top + height) throw new InvalidOperationException("Pointer destination is outside the physical virtual desktop.");
        steps = Math.Max(1, steps);
        for (int step = 1; step <= steps; ++step)
        {
            RequireForeground();
            int px = from.X + (int)Math.Round((x - from.X) * (double)step / steps);
            int py = from.Y + (int)Math.Round((y - from.Y) * (double)step / steps);
            Send(new Input { X = (int)Math.Round((px - left) * 65535.0 / (width - 1)), Y = (int)Math.Round((py - top) * 65535.0 / (height - 1)), MouseFlags = 0xC001 });
            // Pacing pointer movement is not used as a completion oracle.
            Thread.Sleep(12);
        }
    }
    public void Begin(uint pid, int x, int y, bool copy)
    {
        AssertIdleInput(); Activate(pid); RequirePoint(x, y, pid);
        sourceProcess = pid;
        MoveTo(x, y, 1);
        if (copy) { RequireForeground(); Key(0x11, false); controlDown = true; }
        RequireForeground(); RequirePoint(x, y, pid);
        Send(new Input { MouseFlags = 2 }); mouseDown = true;
    }
    public void End(uint pid, int x, int y)
    {
        RequireForeground(); RequirePoint(x, y, pid);
        Point actual;
        if (!GetCursorPos(out actual) || Math.Abs(actual.X - x) > 1 || Math.Abs(actual.Y - y) > 1) throw new InvalidOperationException("Cursor moved unexpectedly before mouse release.");
        RequirePoint(actual.X, actual.Y, pid);
        if (!mouseDown) throw new InvalidOperationException("Driver does not own a mouse press.");
        Send(new Input { MouseFlags = 4 }); mouseDown = false;
    }
    public void Escape()
    {
        RequireForeground();
        if (!escapeDown) { Key(0x1B, false); escapeDown = true; }
    }
    public void ReleaseEscape() { if (escapeDown) { Key(0x1B, true); escapeDown = false; } }
    public void ReleaseControl()
    {
        if (controlDown) { Key(0x11, true); controlDown = false; }
    }
    public void ReleaseOwnedKeys()
    {
        try { ReleaseEscape(); }
        finally { ReleaseControl(); }
    }
    public void ReleaseOwnedInput(Process exitedSource)
    {
        try
        {
            if (mouseDown)
            {
                // Sending Escape is not evidence that OLE observed cancellation.
                // The driver retains this source handle before injecting input
                // and waits for its exit before allowing an emergency button-up.
                if (exitedSource == null || (uint)exitedSource.Id != sourceProcess || !exitedSource.HasExited)
                    throw new InvalidOperationException("Mouse release requires the exact retained drag source to have exited.");
                Send(new Input { MouseFlags = 4 }); mouseDown = false;
            }
        }
        finally { ReleaseOwnedKeys(); }
    }
    public void ReleaseOwnedInput() { ReleaseOwnedInput(null); }
    public void Dispose() { try { ReleaseOwnedInput(); } finally { SetThreadDpiAwarenessContext(priorDpi); } }
}
