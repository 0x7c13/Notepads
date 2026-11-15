// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// This narrow external probe preserves supported clipboard payload bytes before
// a canary is written. Opaque object/handle formats are refused before mutation.
public sealed class MarkdownClipboardLease : IDisposable
{
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateWindowEx(uint exStyle, string className, string title, uint style,
        int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr parameter);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool DestroyWindow(IntPtr window);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool OpenClipboard(IntPtr window);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool CloseClipboard();
    [DllImport("user32.dll", SetLastError = true)] private static extern uint EnumClipboardFormats(uint format);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr GetClipboardData(uint format);
    [DllImport("user32.dll", SetLastError = true)] private static extern IntPtr SetClipboardData(uint format, IntPtr memory);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool EmptyClipboard();
    [DllImport("user32.dll")] private static extern uint GetClipboardSequenceNumber();
    [DllImport("user32.dll")] private static extern IntPtr GetClipboardOwner();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetClipboardFormatName(uint format, StringBuilder name, int count);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern UIntPtr GlobalSize(IntPtr memory);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern IntPtr GlobalLock(IntPtr memory);
    [DllImport("kernel32.dll")] private static extern bool GlobalUnlock(IntPtr memory);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern IntPtr GlobalAlloc(uint flags, UIntPtr count);
    [DllImport("kernel32.dll")] private static extern IntPtr GlobalFree(IntPtr memory);

    private sealed class Saved { public uint Format; public byte[] Bytes; }
    private readonly List<Saved> saved = new List<Saved>();
    private readonly IntPtr testWindow;
    private IntPtr clipboardWindow;
    private uint sequence;
    private bool changed, restored, pendingOwnedMutation;
    public int PreservedFormats { get { return saved.Count; } }
    public bool Restored { get { return restored; } }

    public MarkdownClipboardLease(IntPtr ownedWindow)
    {
        if (ownedWindow == IntPtr.Zero) throw new ArgumentException("An exact owned test HWND is required.");
        testWindow = ownedWindow;
        // The isolated app can exit before the driver restores the clipboard.
        // Keep the write owner on this driver thread, independent of that app.
        clipboardWindow = CreateWindowEx(0, "STATIC", null, 0, 0, 0, 0, 0,
            new IntPtr(-3), IntPtr.Zero, IntPtr.Zero, IntPtr.Zero); // HWND_MESSAGE
        if (clipboardWindow == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try
        {
            Open();
            try
            {
                long total = 0;
                for (uint format = EnumClipboardFormats(0); format != 0; format = EnumClipboardFormats(format))
                {
                    if (!CanClone(format)) throw new InvalidOperationException("Clipboard format cannot be preserved by this text probe: " + Name(format) + ". Nothing was written.");
                    var data = GetClipboardData(format);
                    var size = GlobalSize(data).ToUInt64();
                    if (data == IntPtr.Zero || size == 0 || size > 32 * 1024 * 1024 || (total += (long)size) > 32 * 1024 * 1024)
                        throw new InvalidOperationException("Existing clipboard payload cannot be safely retained. Nothing was written.");
                    saved.Add(new Saved { Format = format, Bytes = Read(data, checked((int)size)) });
                }
                sequence = GetClipboardSequenceNumber();
            }
            finally { CloseClipboard(); }
        }
        catch
        {
            DestroyWindow(clipboardWindow);
            throw;
        }
    }

    public void SetText(string text)
        => WriteText(text, null, false);

    // The native paste path can cause Windows to materialize derived formats.
    // Reacquire ownership only after exact-value verification under the same
    // clipboard lock as the write; a changed sequence alone never authorizes it.
    public void SetTextAfterVerifiedText(string text, string expectedCurrent)
        => WriteText(text, expectedCurrent, true);

    private void WriteText(string text, string expectedCurrent, bool verifyText)
    {
        Open();
        try
        {
            if (verifyText) RequireText(expectedCurrent);
            else RequireSequence();
            var bytes = Encoding.Unicode.GetBytes(text + "\0");
            var memory = Allocate(bytes);
            try
            {
                EmptyLocked();
                if (SetClipboardData(13, memory) == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
                memory = IntPtr.Zero;
                sequence = GetClipboardSequenceNumber();
                pendingOwnedMutation = false;
            }
            catch (Exception writeError)
            {
                if (pendingOwnedMutation)
                {
                    try { RestoreLocked(); }
                    catch (Exception restoreError)
                    {
                        throw new AggregateException("The test clipboard write and its restoration both failed; cleanup can retry the last exact owned state.", writeError, restoreError);
                    }
                }
                throw;
            }
            finally { if (memory != IntPtr.Zero) GlobalFree(memory); }
        }
        finally { CloseClipboard(); }
    }

    // Adopt only a verified test result. A canary surviving Ctrl+C is not PASS.
    public bool MatchesText(string expected)
    {
        Open();
        try { return MatchesTextLocked(expected); }
        finally { CloseClipboard(); }
    }

    public void Dispose()
    {
        try { Restore(null, false); }
        finally
        {
            if (clipboardWindow != IntPtr.Zero)
            {
                if (!DestroyWindow(clipboardWindow)) throw new Win32Exception(Marshal.GetLastWin32Error());
                clipboardWindow = IntPtr.Zero;
            }
        }
    }

    public void RestoreAfterVerifiedText(string expectedCurrent)
        => Restore(expectedCurrent, true);

    public void RestoreAfterTestCopy(string lastVerifiedText, uint testProcessId, bool testProcessAlive)
        => Restore(lastVerifiedText, true, testProcessId, testProcessAlive);

    private void Restore(string expectedCurrent, bool verifyText, uint testProcessId = 0, bool testProcessAlive = false)
    {
        if (restored) return;
        if (!changed) { restored = true; return; }
        Open();
        try
        {
            // An interrupted own write/restore has already removed the canary.
            // Its exact mutation sequence is the only authority for a retry.
            if (pendingOwnedMutation) RequireSequence();
            else if (verifyText)
            {
                if (!IsOwnedByTestProcess(testProcessId, testProcessAlive)) RequireText(expectedCurrent);
            }
            else RequireSequence();
            RestoreLocked();
        }
        finally { CloseClipboard(); }
    }

    // A failed copy can still have replaced the clipboard. Trust the exact
    // test window or another owner window from the live isolated test process.
    private bool IsOwnedByTestProcess(uint testProcessId, bool testProcessAlive)
    {
        if (!testProcessAlive || testProcessId == 0) return false;
        var owner = GetClipboardOwner();
        return owner != IntPtr.Zero && (owner == testWindow ||
            (GetWindowThreadProcessId(owner, out uint processId) != 0 && processId == testProcessId));
    }

    private void EmptyLocked()
    {
        if (!EmptyClipboard()) throw new Win32Exception(Marshal.GetLastWin32Error());
        changed = true;
        pendingOwnedMutation = true;
        sequence = GetClipboardSequenceNumber();
    }

    private void RestoreLocked()
    {
        var allocations = new List<IntPtr>();
        try
        {
            foreach (var item in saved) allocations.Add(Allocate(item.Bytes));
            EmptyLocked();
            for (var index = 0; index < saved.Count; index++)
            {
                if (SetClipboardData(saved[index].Format, allocations[index]) == IntPtr.Zero)
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                allocations[index] = IntPtr.Zero;
                sequence = GetClipboardSequenceNumber();
            }
            foreach (var item in saved)
            {
                var data = GetClipboardData(item.Format);
                if (data == IntPtr.Zero || GlobalSize(data).ToUInt64() != (ulong)item.Bytes.Length ||
                    !Read(data, item.Bytes.Length).SequenceEqual(item.Bytes))
                    throw new InvalidOperationException("Clipboard restoration byte verification failed.");
            }
            restored = true;
            pendingOwnedMutation = false;
        }
        finally { foreach (var allocation in allocations) if (allocation != IntPtr.Zero) GlobalFree(allocation); }
    }

    private bool MatchesTextLocked(string expected)
    {
        var data = GetClipboardData(13);
        if (data == IntPtr.Zero) return false;
        var size = GlobalSize(data).ToUInt64();
        if (size == 0 || size > 32 * 1024 * 1024) return false;
        var bytes = Read(data, checked((int)size));
        var text = Encoding.Unicode.GetString(bytes);
        var end = text.IndexOf('\0');
        if (end >= 0) text = text.Substring(0, end);
        if (!string.Equals(text, expected, StringComparison.Ordinal)) return false;
        var currentSequence = GetClipboardSequenceNumber();
        if (pendingOwnedMutation && currentSequence != sequence) return false;
        sequence = currentSequence;
        return true;
    }

    private void RequireText(string expected)
    {
        if (expected == null || !MatchesTextLocked(expected))
            throw new InvalidOperationException("Clipboard no longer contains the exact verified test text; it was left untouched.");
    }

    private void RequireSequence()
    {
        if (GetClipboardSequenceNumber() != sequence)
            throw new InvalidOperationException("Clipboard changed outside the last verified test value; it was left untouched.");
    }

    private void Open()
    {
        if (clipboardWindow == IntPtr.Zero) throw new ObjectDisposedException(nameof(MarkdownClipboardLease));
        var lastError = 0;
        for (var attempt = 0; attempt < 50; attempt++)
        {
            if (OpenClipboard(clipboardWindow)) return;
            lastError = Marshal.GetLastWin32Error();
            Thread.Sleep(10);
        }
        throw new Win32Exception(lastError, "The clipboard could not be opened.");
    }

    private static bool CanClone(uint format)
    {
        if (format == 1 || format == 7 || format == 8 || format == 13 || format == 15 || format == 16 || format == 17) return true;
        return format >= 0xc000 && new[] { "HTML Format", "Rich Text Format", "Rich Text Format Without Objects", "CSV", "PNG", "JFIF", "image/png", "CanIncludeInClipboardHistory", "CanUploadToCloudClipboard", "ExcludeClipboardContentFromMonitorProcessing" }.Contains(Name(format));
    }

    private static string Name(uint format)
    {
        var name = new StringBuilder(256);
        return GetClipboardFormatName(format, name, name.Capacity) > 0 ? name.ToString() : format.ToString();
    }

    private static byte[] Read(IntPtr memory, int count)
    {
        var pointer = GlobalLock(memory);
        if (pointer == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try { var bytes = new byte[count]; Marshal.Copy(pointer, bytes, 0, count); return bytes; }
        finally { GlobalUnlock(memory); }
    }

    private static IntPtr Allocate(byte[] bytes)
    {
        var memory = GlobalAlloc(2, (UIntPtr)(uint)bytes.Length);
        if (memory == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        var pointer = GlobalLock(memory);
        if (pointer == IntPtr.Zero) { GlobalFree(memory); throw new Win32Exception(Marshal.GetLastWin32Error()); }
        try { Marshal.Copy(bytes, 0, pointer, bytes.Length); }
        finally { GlobalUnlock(memory); }
        return memory;
    }
}
