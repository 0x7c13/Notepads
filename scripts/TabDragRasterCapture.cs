// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;

// Observe DWM output independently of XAML's temporarily cached transforms.
// Only the exact retained process's unobscured foreground window is captured.
public sealed class TabDragRasterCapture
{
    public sealed class Frame
    {
        public double Milliseconds { get; set; }
        public double[] X { get; set; }
        public int[] Pixels { get; set; }
    }
    [StructLayout(LayoutKind.Sequential)] private struct Point { public int X, Y; }
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flag);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    private static readonly int[] Colors = { 0x16E74D, 0xF521D3, 0x1DADF9 };
    private readonly Task capture;
    private readonly ManualResetEventSlim ready = new ManualResetEventSlim();
    private readonly List<Frame> frames = new List<Frame>();
    private readonly Stopwatch watch = Stopwatch.StartNew();
    private Frame latest;
    public Frame Release { get; private set; }
    public double ReleaseSampleMilliseconds { get; private set; }
    public Frame[] Frames { get { lock (frames) return frames.ToArray(); } }

    public TabDragRasterCapture(Process owner, int x, int y, int width, int height, string folder)
    {
        if (width < 20 || height < 6 || width > 4000 || height > 300) throw new ArgumentOutOfRangeException();
        capture = Task.Factory.StartNew(() =>
        {
            var prior = SetThreadDpiAwarenessContext(new IntPtr(-4));
            var images = new List<Bitmap>();
            try
            {
                if (prior == IntPtr.Zero) throw new InvalidOperationException("Physical-coordinate capture is unavailable.");
                while (watch.ElapsedMilliseconds < 650)
                {
                    if (owner.HasExited) throw new InvalidOperationException("The retained capture owner exited.");
                    foreach (var point in new[] { new Point { X = x + 2, Y = y + 2 }, new Point { X = x + width - 3, Y = y + 2 },
                        new Point { X = x + 2, Y = y + height - 3 }, new Point { X = x + width - 3, Y = y + height - 3 } })
                    {
                        var window = WindowFromPoint(point);
                        GetWindowThreadProcessId(window, out uint pid);
                        if (pid != owner.Id || GetAncestor(window, 2) != GetAncestor(GetForegroundWindow(), 2))
                            throw new InvalidOperationException("The owned tab capture is occluded or lost foreground.");
                    }
                    var bitmap = new Bitmap(width, height, PixelFormat.Format32bppArgb);
                    images.Add(bitmap);
                    using (var graphics = Graphics.FromImage(bitmap)) graphics.CopyFromScreen(x, y, 0, 0, new Size(width, height));
                    var frame = new Frame { Milliseconds = watch.Elapsed.TotalMilliseconds };
                    Locate(bitmap, frame);
                    lock (frames) { frames.Add(frame); latest = frame; }
                    ready.Set();
                    Thread.Sleep(4); // Sampling pace, never a correctness oracle.
                }
                Directory.CreateDirectory(folder);
                for (int index = 0; index < images.Count; index++)
                    images[index].Save(Path.Combine(folder, index.ToString("D3") + ".png"), ImageFormat.Png);
            }
            finally
            {
                ready.Set();
                foreach (var image in images) image.Dispose();
                if (prior != IntPtr.Zero) SetThreadDpiAwarenessContext(prior);
            }
        }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
        if (!ready.Wait(5000)) throw new TimeoutException("No owned tab capture frame.");
        if (capture.IsCompleted) capture.GetAwaiter().GetResult();
    }

    private static void Locate(Bitmap bitmap, Frame frame)
    {
        var data = bitmap.LockBits(new Rectangle(0, 0, bitmap.Width, bitmap.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            var bytes = new byte[data.Stride * data.Height];
            Marshal.Copy(data.Scan0, bytes, 0, bytes.Length);
            var sums = new long[3];
            var counts = new int[3];
            var minimum = new[] { int.MaxValue, int.MaxValue, int.MaxValue };
            var maximum = new[] { -1, -1, -1 };
            for (int row = 0; row < data.Height; row++)
                for (int column = 0; column < bitmap.Width; column++)
                {
                    int offset = row * data.Stride + column * 4;
                    int color = bytes[offset + 2] << 16 | bytes[offset + 1] << 8 | bytes[offset];
                    for (int index = 0; index < Colors.Length; index++)
                        if (color == Colors[index])
                        {
                            sums[index] += column;
                            counts[index]++;
                            minimum[index] = Math.Min(minimum[index], column);
                            maximum[index] = Math.Max(maximum[index], column);
                        }
                }
            var result = new double[3];
            for (int index = 0; index < result.Length; index++)
                result[index] = counts[index] >= 20 && maximum[index] - minimum[index] >= 4
                    ? (double)sums[index] / counts[index] : double.NaN;
            frame.X = result;
            frame.Pixels = counts;
        }
        finally { bitmap.UnlockBits(data); }
    }

    public void WaitForSource(int source, double x)
    {
        while (true)
        {
            lock (frames) if (latest != null && Math.Abs(latest.X[source] - x) <= 1) return;
            if (capture.IsCompleted) { capture.GetAwaiter().GetResult(); throw new InvalidOperationException("The source never rendered at the injected pointer position."); }
            Thread.Sleep(2);
        }
    }

    public void MarkRelease()
    {
        lock (frames)
        {
            // Preserve the last presented position but date the release request
            // now, not at the earlier screenshot. End queues input next; neither
            // timestamp claims to observe Windows dispatch or presentation.
            ReleaseSampleMilliseconds = latest.Milliseconds;
            Release = new Frame
            {
                Milliseconds = watch.Elapsed.TotalMilliseconds,
                X = (double[])latest.X.Clone(), Pixels = (int[])latest.Pixels.Clone()
            };
        }
    }

    public static void RequireSettlementSamples(Frame release, Frame[] samples, int marker, double jumpPixels, double tolerancePixels)
    {
        // A sample is useful only while a full jump still exceeds the existing
        // movement oracle's allowance. This depends on displacement/tolerance,
        // not on whether this desktop happens to produce 30 or 60 samples/sec.
        bool hasBaseline = release.X != null && marker >= 0 && marker < release.X.Length &&
            double.IsFinite(release.X[marker]);
        foreach (var sample in samples)
            if (hasBaseline && sample.Milliseconds > release.Milliseconds && sample.Milliseconds < release.Milliseconds + 90 &&
                sample.X != null && marker >= 0 && marker < sample.X.Length && double.IsFinite(sample.X[marker]))
            {
                double remaining = Math.Pow(1 - (sample.Milliseconds - release.Milliseconds) / 90, 3);
                if (double.IsFinite(jumpPixels) && double.IsFinite(tolerancePixels) && tolerancePixels >= 0 &&
                    jumpPixels * remaining > tolerancePixels) return;
            }
        throw new InvalidOperationException("No informative sample during the 90 ms settlement; animation correctness is unverified.");
    }

    public void Finish()
    {
        try { capture.GetAwaiter().GetResult(); }
        finally { ready.Dispose(); }
    }
}
