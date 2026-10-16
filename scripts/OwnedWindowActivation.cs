// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

// Startup only: acquire an owned test window before any fixture or held input.
// Ordinary foreground APIs may be denied by Windows. The fallback clicks only
// a verified non-client caption, then restores its temporary Z-order class.
// Running test cases must keep their existing fail-closed foreground guards.
public static class OwnedWindowActivation
{
    [StructLayout(LayoutKind.Sequential)] private struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] private struct TitleBarInfo
    {
        public int Size;
        public Rect Rect;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 6)] public uint[] State;
    }
    [StructLayout(LayoutKind.Explicit, Size = 40)] private struct Input
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public int X;
        [FieldOffset(12)] public int Y;
        [FieldOffset(20)] public uint Flags;
    }
    private delegate bool EnumWindow(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(IntPtr root, EnumWindow callback, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")] private static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] private static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] private static extern bool GetTitleBarInfo(IntPtr window, ref TitleBarInfo info);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] private static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] private static extern uint GetDoubleClickTime();
    [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, Input[] input, int size);

    private const uint PositionFlags = 0x213; // NOMOVE | NOSIZE | NOACTIVATE | NOOWNERZORDER
    private static uint ProcessOf(IntPtr window) { GetWindowThreadProcessId(window, out uint process); return process; }
    private static bool Topmost(IntPtr window) => (GetWindowLongPtrW(window, -20).ToInt64() & 8) != 0;
    private static bool IsIdle(bool allowLeftButton = false)
    {
        for (int key = 1; key <= 254; key++)
            if (!(allowLeftButton && key == 1) && GetAsyncKeyState(key) < 0) return false;
        return true;
    }
    public static void RequireIdle() => RequireIdle(false);
    private static void RequireIdle(bool allowLeftButton)
    {
        if (!IsIdle(allowLeftButton)) throw new InvalidOperationException("Physical input is held; refusing foreground acquisition.");
    }
    private static void RequireIdentity(uint process, IntPtr root, uint rootProcess, IntPtr anchor, bool requireVisible = true)
    {
        if (!IsWindow(root) || ProcessOf(root) != rootProcess || !IsWindow(anchor) ||
            ProcessOf(anchor) != process || GetAncestor(anchor, 2) != root ||
            (requireVisible && (!IsWindowVisible(root) || !IsWindowVisible(anchor))))
            throw new InvalidOperationException("The exact owned test window changed during foreground acquisition.");
    }
    private static void RequireIndependentRoot(IntPtr root)
    {
        bool hasOwnedWindow = false;
        EnumWindows((window, _) => { hasOwnedWindow |= GetWindow(window, 4) == root; return !hasOwnedWindow; }, IntPtr.Zero);
        if (GetWindow(root, 4) != IntPtr.Zero || hasOwnedWindow)
            throw new InvalidOperationException("Temporary foreground promotion requires a root without owner or owned top-level windows.");
    }
    private static void Raise(IntPtr root, IntPtr after)
    {
        if (!SetWindowPos(root, after, 0, 0, 0, 0, PositionFlags))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not raise the owned test window.");
    }
    private static bool CaptionAt(IntPtr root, Point point, bool requireVisible)
    {
        if (point.X < short.MinValue || point.X > short.MaxValue || point.Y < short.MinValue || point.Y > short.MaxValue)
            return false; // WM_NCHITTEST coordinates are signed 16-bit values.
        var packed = new IntPtr(unchecked((int)((ushort)point.X | ((uint)(ushort)point.Y << 16))));
        if (requireVisible)
        {
            var actual = WindowFromPoint(point);
            if (GetAncestor(actual, 2) != root) return false;
            // A parent's caption classification cannot authorize clicking a
            // button or client control which covers that same physical point.
            if (actual != root && (SendMessageTimeoutW(actual, 0x84, IntPtr.Zero, packed, 0x22, 100, out var childHit) == IntPtr.Zero ||
                childHit.ToInt64() != 2)) return false;
        }
        return SendMessageTimeoutW(root, 0x84, IntPtr.Zero, packed, 0x22, 100, out var hit) != IntPtr.Zero && hit.ToInt64() == 2;
    }
    private static Point FindCaption(IntPtr root)
    {
        var title = new TitleBarInfo { Size = Marshal.SizeOf(typeof(TitleBarInfo)), State = new uint[6] };
        if (!GetTitleBarInfo(root, ref title) || title.Rect.Right <= title.Rect.Left || title.Rect.Bottom <= title.Rect.Top)
            throw new InvalidOperationException("The owned window has no available non-client caption.");
        foreach (int quarter in new[] { 2, 1, 3 })
        {
            var point = new Point { X = title.Rect.Left + (title.Rect.Right - title.Rect.Left) * quarter / 4,
                Y = title.Rect.Top + (title.Rect.Bottom - title.Rect.Top) / 2 };
            if (CaptionAt(root, point, false)) return point;
        }
        throw new InvalidOperationException("No safe caption point exists in the owned test window.");
    }
    private static void Send(Input input)
    {
        if (SendInput(1, new[] { input }, 40) != 1)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Foreground acquisition input failed.");
    }

    public static string Acquire(uint process, IntPtr root)
    {
        IntPtr anchor = ProcessOf(root) == process ? root : IntPtr.Zero;
        if (root != IntPtr.Zero && anchor == IntPtr.Zero)
            EnumChildWindows(root, (child, _) => {
                if (ProcessOf(child) == process && IsWindowVisible(child)) { anchor = child; return false; }
                return true;
            }, IntPtr.Zero);
        return Acquire(process, root, anchor);
    }

    public static string Acquire(uint process, IntPtr root, IntPtr anchor)
    {
        if (IntPtr.Size != 8 || process == 0 || root == IntPtr.Zero || anchor == IntPtr.Zero)
            throw new ArgumentException("Foreground acquisition requires a 64-bit driver and exact owned window handles.");
        uint rootProcess = ProcessOf(root);
        RequireIdentity(process, root, rootProcess, anchor);
        RequireIdle();
        bool wasTopmost = Topmost(root);
        IntPtr initialForeground = GetAncestor(GetForegroundWindow(), 2);
        IntPtr dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (dpi == IntPtr.Zero) throw new InvalidOperationException("Could not establish physical-coordinate DPI awareness.");
        bool promoted = false, mouseDown = false, moved = false, acquired = false;
        Point priorCursor = default, injectedCursor = default;
        var clock = Stopwatch.StartNew();
        var deadline = TimeSpan.FromMilliseconds(3000 + GetDoubleClickTime());
        void CheckSetup(bool allowLeftButton = false)
        {
            RequireIdentity(process, root, rootProcess, anchor);
            RequireIdle(allowLeftButton);
            var foreground = GetAncestor(GetForegroundWindow(), 2);
            if (foreground != IntPtr.Zero && foreground != initialForeground && foreground != root)
                throw new InvalidOperationException("Foreground moved to another window during test setup.");
            if (clock.Elapsed > deadline) throw new TimeoutException("Owned foreground acquisition timed out.");
        }
        try
        {
            if (IsIconic(root)) ShowWindowAsync(root, 9);
            Raise(root, IntPtr.Zero);
            SetForegroundWindow(root);
            while (GetAncestor(GetForegroundWindow(), 2) != root && clock.ElapsedMilliseconds < 500)
            {
                CheckSetup();
                Thread.Sleep(15);
            }
            CheckSetup();
            if (GetAncestor(GetForegroundWindow(), 2) == root)
            {
                acquired = true;
                return "foreground-api";
            }

            // A single caption click must not combine with a preceding user
            // click and maximize the window. Only this fallback waits.
            long clickAfter = clock.ElapsedMilliseconds + GetDoubleClickTime();
            while (clock.ElapsedMilliseconds <= clickAfter) { CheckSetup(); Thread.Sleep(15); }
            var caption = FindCaption(root);
            if (!CaptionAt(root, caption, true))
            {
                CheckSetup();
                RequireIndependentRoot(root);
                if (Topmost(root)) throw new InvalidOperationException("An already-topmost owned caption is occluded.");
                promoted = true;
                Raise(root, new IntPtr(-1));
                caption = FindCaption(root);
            }
            CheckSetup();
            if (!CaptionAt(root, caption, true) || !GetWindowRect(root, out var arranged) || !GetCursorPos(out priorCursor))
                throw new InvalidOperationException("The owned caption is not the actual visible input target.");
            int left = GetSystemMetrics(76), top = GetSystemMetrics(77), width = GetSystemMetrics(78), height = GetSystemMetrics(79);
            if (width < 2 || height < 2 || caption.X < left || caption.Y < top || caption.X >= left + width || caption.Y >= top + height)
                throw new InvalidOperationException("The owned caption is outside the virtual desktop.");
            Send(new Input { X = (int)Math.Round((caption.X - left) * 65535.0 / (width - 1)),
                Y = (int)Math.Round((caption.Y - top) * 65535.0 / (height - 1)), Flags = 0xC001 });
            moved = true;
            if (!GetCursorPos(out injectedCursor) || Math.Abs(injectedCursor.X - caption.X) > 1 ||
                Math.Abs(injectedCursor.Y - caption.Y) > 1 || !CaptionAt(root, injectedCursor, true))
                throw new InvalidOperationException("Pointer moved away from the verified owned caption.");
            CheckSetup();
            Send(new Input { Flags = 2 }); mouseDown = true;
            try
            {
                CheckSetup(allowLeftButton: true);
                if (!GetCursorPos(out var cursor) || cursor.X != injectedCursor.X || cursor.Y != injectedCursor.Y || !CaptionAt(root, cursor, true))
                    throw new InvalidOperationException("The caption click was interrupted.");
            }
            finally { Send(new Input { Flags = 4 }); mouseDown = false; }
            // SendInput inserts events into a queue. Observe delivery of our
            // release before resuming the ordinary all-keys-idle guard.
            long releaseDeadline = clock.ElapsedMilliseconds + 250;
            while (GetAsyncKeyState(1) < 0 && clock.ElapsedMilliseconds < releaseDeadline)
            {
                CheckSetup(allowLeftButton: true);
                Thread.Sleep(1);
            }
            while (GetAncestor(GetForegroundWindow(), 2) != root) { CheckSetup(); Thread.Sleep(15); }
            CheckSetup();
            if (!GetWindowRect(root, out var after) || after.Left != arranged.Left || after.Top != arranged.Top ||
                after.Right != arranged.Right || after.Bottom != arranged.Bottom)
                throw new InvalidOperationException("Foreground acquisition unexpectedly changed the arranged window bounds.");
            acquired = true;
            return promoted ? "temporary-topmost-caption" : "caption";
        }
        finally
        {
            try
            {
                if (mouseDown) Send(new Input { Flags = 4 });
            }
            finally
            {
                try
                {
                    if (promoted)
                    {
                        // Hiding/minimizing the same window must not prevent
                        // cleanup of the temporary topmost flag.
                        RequireIdentity(process, root, rootProcess, anchor, requireVisible: false);
                        Raise(root, new IntPtr(-2));
                        if (Topmost(root)) throw new InvalidOperationException("Could not restore the owned window's ordinary Z-order class.");
                    }
                    // Leave real user motion alone. SetCursorPos restores only
                    // the pointer, without injecting input into another window.
                    if (moved && GetCursorPos(out var cursor) && cursor.X == injectedCursor.X && cursor.Y == injectedCursor.Y)
                    {
                        if (IsIdle()) SetCursorPos(priorCursor.X, priorCursor.Y);
                    }
                    if (acquired)
                    {
                        RequireIdentity(process, root, rootProcess, anchor);
                        RequireIdle();
                        if (Topmost(root) != wasTopmost)
                            throw new InvalidOperationException("Foreground acquisition changed the owned window's Z-order class.");
                        if (GetAncestor(GetForegroundWindow(), 2) != root)
                            throw new InvalidOperationException("The owned window lost foreground while completing startup acquisition.");
                    }
                }
                finally
                {
                    if (SetThreadDpiAwarenessContext(dpi) == IntPtr.Zero)
                        throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not restore the driver's DPI awareness.");
                }
            }
        }
    }
}
