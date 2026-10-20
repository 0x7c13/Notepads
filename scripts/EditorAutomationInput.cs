// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

// External desktop observations for the isolated editor. Coordinates are actual
// screen pixels. Never inject into another process or capture an occluded editor.
public static class EditorAutomationInput
{
    [StructLayout(LayoutKind.Sequential)] private struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Explicit, Size = 40)] private struct Input
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public int X;
        [FieldOffset(12)] public int Y;
        [FieldOffset(16)] public uint MouseData;
        [FieldOffset(20)] public uint MouseFlags;
        [FieldOffset(8)] public ushort VirtualKey;
        [FieldOffset(12)] public uint KeyFlags;
    }
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", SetLastError = true)] private static extern uint SendInput(uint count, Input[] input, int size);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool SystemParametersInfoW(uint action, uint parameter, out uint value, uint flags);

    public static uint WheelLines() => WheelUnits(0x0068);
    public static uint WheelCharacters() => WheelUnits(0x006c);

    private static uint WheelUnits(uint action)
    {
        if (!SystemParametersInfoW(action, 0, out uint lines, 0))
            throw new InvalidOperationException("Could not read the system wheel preference.");
        return lines;
    }

    private static void RequirePoint(uint process, int x, int y)
    {
        var window = WindowFromPoint(new Point { X = x, Y = y });
        GetWindowThreadProcessId(window, out uint actual);
        if (actual != process || GetAncestor(window, 2) != GetForegroundWindow())
            throw new InvalidOperationException("The isolated editor is not the foreground hit target.");
    }

    public static void RequireIdle()
    {
        for (int key = 1; key <= 254; key++)
            if (GetAsyncKeyState(key) < 0) throw new InvalidOperationException("Physical input is held; refusing test input.");
    }

    public static void RequireForeground(uint process, int x, int y)
    {
        RequireIdle();
        var dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (dpi == IntPtr.Zero) throw new InvalidOperationException("Could not establish physical-coordinate DPI awareness.");
        try
        {
            // Reuse the input hit-target contract: the child HWND belongs to
            // the app, while its foreground root may belong to AppFrameHost.
            RequirePoint(process, x, y);
        }
        finally { SetThreadDpiAwarenessContext(dpi); }
    }

    public static void Click(uint process, int x, int y) => ClickButton(process, x, y, false);
    public static void RightClick(uint process, int x, int y) => ClickButton(process, x, y, true);

    public static void Wheel(uint process, int x, int y, int delta, bool horizontal = false, bool shift = false)
    {
        Move(process, x, y);
        RequireIdle();
        var dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (dpi == IntPtr.Zero) throw new InvalidOperationException("Could not establish physical-coordinate DPI awareness.");
        try
        {
            RequirePoint(process, x, y);
            if (!GetCursorPos(out var point) || Math.Abs(point.X - x) > 1 || Math.Abs(point.Y - y) > 1)
                throw new InvalidOperationException("The pointer left the owned wheel target.");
            var input = new Input { MouseData = unchecked((uint)delta), MouseFlags = horizontal ? 0x1000u : 0x0800u };
            var release = new Input { Type = 1, VirtualKey = 0x10, KeyFlags = 2 };
            var inputs = shift ? new[] { new Input { Type = 1, VirtualKey = 0x10 }, input, release } : new[] { input };
            bool releaseShift = shift;
            try
            {
                if (SendInput((uint)inputs.Length, inputs, 40) != inputs.Length)
                    throw new InvalidOperationException("Mouse-wheel injection failed.");
                releaseShift = false;
            }
            finally
            {
                if (releaseShift && SendInput(1, new[] { release }, 40) != 1)
                    throw new InvalidOperationException("Shift cleanup after mouse-wheel injection failed.");
            }
        }
        finally { SetThreadDpiAwarenessContext(dpi); }
    }

    public static void Move(uint process, int x, int y)
    {
        if (IntPtr.Size != 8) throw new PlatformNotSupportedException("The editor driver requires a 64-bit process.");
        RequireIdle();
        var dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        try
        {
            RequirePoint(process, x, y);
            int left = GetSystemMetrics(76), top = GetSystemMetrics(77);
            int width = GetSystemMetrics(78), height = GetSystemMetrics(79);
            if (width < 2 || height < 2 || x < left || y < top || x >= left + width || y >= top + height)
                throw new InvalidOperationException("Pointer destination is outside the physical virtual desktop.");
            // SetCursorPos alone does not produce UWP PointerMoved here. Use
            // the same absolute virtual-desktop mouse input as the OLE driver.
            var input = new Input {
                X = (int)Math.Round((x - left) * 65535.0 / (width - 1)),
                Y = (int)Math.Round((y - top) * 65535.0 / (height - 1)),
                MouseFlags = 0xC001
            };
            if (SendInput(1, new[] { input }, 40) != 1)
                throw new InvalidOperationException("Mouse-move injection failed.");
            // Normalized absolute coordinates can round to the adjacent pixel.
            if (!GetCursorPos(out var point) || Math.Abs(point.X - x) > 1 || Math.Abs(point.Y - y) > 1)
                throw new InvalidOperationException("The physical pointer moved away from the test destination.");
            RequirePoint(process, point.X, point.Y);
        }
        finally { SetThreadDpiAwarenessContext(dpi); }
    }

    private static void ClickButton(uint process, int x, int y, bool right)
    {
        if (IntPtr.Size != 8) throw new PlatformNotSupportedException("The editor driver requires a 64-bit process.");
        RequireIdle();
        var dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        bool down = false;
        try
        {
            RequirePoint(process, x, y);
            // Move and button events must share the real input queue. Moving
            // only the cursor leaves UWP's last pointer location stale.
            Move(process, x, y);
            if (!GetCursorPos(out var point) || Math.Abs(point.X - x) > 1 || Math.Abs(point.Y - y) > 1)
                throw new InvalidOperationException("Could not position the test pointer.");
            var destination = point;
            RequirePoint(process, destination.X, destination.Y);
            if (SendInput(1, new[] { new Input { MouseFlags = right ? 8u : 2u } }, 40) != 1)
                throw new InvalidOperationException("Mouse-down injection failed.");
            down = true;
            if (!GetCursorPos(out point) || point.X != destination.X || point.Y != destination.Y)
                throw new InvalidOperationException("The physical pointer moved during the test click.");
            RequirePoint(process, destination.X, destination.Y);
        }
        finally
        {
            try
            {
                if (down && SendInput(1, new[] { new Input { MouseFlags = right ? 16u : 4u } }, 40) != 1)
                    throw new InvalidOperationException("Mouse-up injection failed.");
            }
            finally { SetThreadDpiAwarenessContext(dpi); }
        }
    }

    public static EditorInkSnapshot Capture(uint process, int left, int top, int width, int height, string path) =>
        Capture(process, left, top, width, height, path, IntPtr.Zero);

    public static EditorInkSnapshot Capture(uint process, int left, int top, int width, int height, string path,
        IntPtr dragSourceWindow)
    {
        if (width < 20 || height < 20 || width > 4000 || height > 2000)
            throw new ArgumentOutOfRangeException(nameof(width));
        var dpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        try
        {
            foreach (var point in new[] {
                new Point { X = left + 4, Y = top + 4 },
                new Point { X = left + width - 5, Y = top + 4 },
                new Point { X = left + 4, Y = top + height - 5 },
                new Point { X = left + width - 5, Y = top + height - 5 },
                new Point { X = left + width / 2, Y = top + height / 2 } })
            {
                if (dragSourceWindow == IntPtr.Zero)
                    RequirePoint(process, point.X, point.Y);
                else
                {
                    // OLE keeps the verified source window foreground while
                    // the insertion caret is painted in the other owned app.
                    // This exception is observation-only: all input methods
                    // still require the target itself to be foreground.
                    var window = WindowFromPoint(point);
                    GetWindowThreadProcessId(window, out uint actual);
                    if (actual != process || GetForegroundWindow() != dragSourceWindow)
                        throw new InvalidOperationException("The drag source lost foreground or the capture target is occluded.");
                }
            }
            using var image = new Bitmap(width, height, PixelFormat.Format32bppArgb);
            using (var graphics = Graphics.FromImage(image))
                graphics.CopyFromScreen(left, top, 0, 0, new Size(width, height), CopyPixelOperation.SourceCopy);
            if (!string.IsNullOrEmpty(path)) image.Save(path, ImageFormat.Png);
            return new EditorInkSnapshot(image);
        }
        finally { SetThreadDpiAwarenessContext(dpi); }
    }
}

public sealed class EditorInkSnapshot
{
    private readonly byte[] mask;
    private readonly byte[] pixels;
    private readonly int stride, imageWidth, imageHeight;
    public int Left { get; private set; }
    public int Top { get; private set; }
    public int Width { get; private set; }
    public int Height { get; private set; }
    public int InkPixels { get; private set; }
    public ulong Fingerprint { get; private set; }

    internal EditorInkSnapshot(Bitmap image)
    {
        imageWidth = image.Width;
        imageHeight = image.Height;
        var data = image.LockBits(new Rectangle(0, 0, image.Width, image.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            stride = data.Stride;
            if (stride < image.Width * 4) throw new InvalidOperationException("Unsupported screenshot row layout.");
            pixels = new byte[stride * image.Height];
            Marshal.Copy(data.Scan0, pixels, 0, pixels.Length);
        }
        finally { image.UnlockBits(data); }
        int left = image.Width, top = image.Height, right = 0, bottom = 0;
        // Omit the focus outline, but include the native six-pixel text margin.
        for (int y = 3; y < image.Height - 3; ++y)
            for (int x = 3; x < image.Width - 3; ++x)
                if (IsInk(pixels, y * stride + x * 4))
                {
                    left = Math.Min(left, x); top = Math.Min(top, y);
                    right = Math.Max(right, x); bottom = Math.Max(bottom, y);
                    ++InkPixels;
                }
        if (InkPixels < 20) throw new InvalidOperationException("The screenshot contains no visible text fixture.");
        Left = left; Top = top; Width = right - left + 1; Height = bottom - top + 1;
        mask = new byte[Width * Height];
        ulong hash = 14695981039346656037UL;
        for (int y = 0; y < Height; ++y)
            for (int x = 0; x < Width; ++x)
            {
                byte ink = IsInk(pixels, (y + top) * stride + (x + left) * 4) ? (byte)1 : (byte)0;
                mask[y * Width + x] = ink;
            }
        // Pixel stability must also observe selection/background color changes,
        // even when those colors leave the thresholded glyph mask unchanged.
        foreach (var value in pixels) hash = unchecked((hash ^ value) * 1099511628211UL);
        Fingerprint = hash;
    }

    public int PixelRgb(int x, int y)
    {
        if (x < 0 || y < 0 || x >= imageWidth || y >= imageHeight)
            throw new ArgumentOutOfRangeException(nameof(x));
        int offset = y * stride + x * 4;
        return pixels[offset] | pixels[offset + 1] << 8 | pixels[offset + 2] << 16;
    }

    private static bool IsInk(byte[] pixels, int offset) => Math.Min(pixels[offset], Math.Min(pixels[offset + 1], pixels[offset + 2])) < 192;

    public bool SameShape(EditorInkSnapshot other, bool mirror)
    {
        if (Math.Abs(Width - other.Width) > 2 || Math.Abs(Height - other.Height) > 2 ||
            Math.Abs(InkPixels - other.InkPixels) > Math.Max(InkPixels, other.InkPixels) * 0.12) return false;
        return Outside(other, mirror) <= InkPixels * 0.03 && other.Outside(this, mirror) <= other.InkPixels * 0.03;
    }

    private int Outside(EditorInkSnapshot other, bool mirror)
    {
        int outside = 0;
        for (int y = 0; y < Height; ++y)
            for (int x = 0; x < Width; ++x)
            {
                if (mask[y * Width + x] == 0) continue;
                bool found = false;
                int expectedX = mirror ? other.Width - x - 1 : x;
                // Direction alignment can shift the fractional pixel origin.
                // A one-pixel tolerance admits that rasterization difference.
                for (int dy = -1; dy <= 1 && !found; ++dy)
                    for (int dx = -1; dx <= 1 && !found; ++dx)
                    {
                        int ox = expectedX + dx, oy = y + dy;
                        found = ox >= 0 && ox < other.Width && oy >= 0 && oy < other.Height && other.mask[oy * other.Width + ox] != 0;
                    }
                if (!found) ++outside;
            }
        return outside;
    }
}
