// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.ComponentModel;
using System.Linq;
using System.Runtime.InteropServices;

namespace NotepadsEditorTests;

// Desktop-only test setup. Never shipped in the application. It temporarily
// loads the built-in Polish Programmers layout without changing language settings.
internal static partial class KeyboardLayoutTestScope
{
    private static IntPtr _original;
    private static IntPtr _introduced;
    private static partial class NativeMethods
    {
        [LibraryImport("user32.dll")] internal static partial IntPtr GetKeyboardLayout(uint thread);
        [LibraryImport("user32.dll")] internal static unsafe partial int GetKeyboardLayoutList(int count, IntPtr* list);
        [LibraryImport("user32.dll", StringMarshalling = StringMarshalling.Utf16, SetLastError = true)] internal static partial IntPtr LoadKeyboardLayoutW(string id, uint flags);
        [LibraryImport("user32.dll", SetLastError = true)] internal static partial IntPtr ActivateKeyboardLayout(IntPtr layout, uint flags);
        [LibraryImport("user32.dll", SetLastError = true)] internal static partial int UnloadKeyboardLayout(IntPtr layout);
        [LibraryImport("user32.dll")] internal static partial IntPtr GetActiveWindow();
        [LibraryImport("user32.dll")] internal static partial IntPtr GetForegroundWindow();
        [LibraryImport("user32.dll")] internal static partial IntPtr GetAncestor(IntPtr window, uint flags);
    }

    internal static unsafe string Begin()
    {
        if (_original != IntPtr.Zero) throw new InvalidOperationException("A keyboard layout test is already active.");
        var active = NativeMethods.GetActiveWindow();
        if (active == IntPtr.Zero || NativeMethods.GetAncestor(active, 2) != NativeMethods.GetForegroundWindow())
            throw new InvalidOperationException("Keyboard layout setup requires the owned foreground window.");
        var layouts = new IntPtr[NativeMethods.GetKeyboardLayoutList(0, null)];
        fixed (IntPtr* buffer = layouts) NativeMethods.GetKeyboardLayoutList(layouts.Length, buffer);
        _original = NativeMethods.GetKeyboardLayout(0);
        var target = NativeMethods.LoadKeyboardLayoutW("00000415", 0);
        if (target == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        if (!layouts.Contains(target)) _introduced = target;
        if ((target.ToInt64() & 0xffff) != 0x415 || NativeMethods.ActivateKeyboardLayout(target, 0) == IntPtr.Zero)
            throw new InvalidOperationException("The Polish Programmers keyboard layout was not activated.");
        return "keyboardLayout=" + NativeMethods.GetKeyboardLayout(0).ToInt64().ToString("X") + "\n";
    }

    internal static void Restore()
    {
        if (_original == IntPtr.Zero) return;
        if (NativeMethods.ActivateKeyboardLayout(_original, 0) == IntPtr.Zero || NativeMethods.GetKeyboardLayout(0) != _original)
            throw new InvalidOperationException("The original input layout could not be restored.");
        if (_introduced != IntPtr.Zero && NativeMethods.UnloadKeyboardLayout(_introduced) == 0)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "The temporary keyboard layout could not be unloaded.");
        _introduced = _original = IntPtr.Zero;
    }
}
