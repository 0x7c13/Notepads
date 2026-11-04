// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;

// The hosted driver needs elevation to arrange its UWP frame, but the desktop
// drag source must use normal-user rights for Windows to deliver drops to UWP.
public static class CiDesktopPeer
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct StartupInfo
    {
        public int Size;
        public string Reserved, Desktop, Title;
        public int X, Y, Width, Height, XChars, YChars, Fill, Flags;
        public short ShowWindow, ReservedSize;
        public IntPtr ReservedData, Input, Output, Error;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessInfo { public IntPtr Process, Thread; public uint ProcessId, ThreadId; }
    [StructLayout(LayoutKind.Sequential)]
    private struct MandatoryLabel { public IntPtr Sid; public uint Attributes; }
    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool SaferCreateLevel(uint scope, uint level, uint flags, out IntPtr handle, IntPtr reserved);
    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool SaferComputeTokenFromLevel(IntPtr level, IntPtr input, out IntPtr token, uint flags, IntPtr reserved);
    [DllImport("advapi32.dll")]
    private static extern bool SaferCloseLevel(IntPtr level);
    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool SetTokenInformation(IntPtr token, int informationClass, IntPtr information, int length);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcessAsUserW(IntPtr token, string application, StringBuilder command,
        IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint flags,
        IntPtr environment, string directory, ref StartupInfo startup, out ProcessInfo process);
    [DllImport("kernel32.dll")]
    private static extern bool TerminateProcess(IntPtr process, int code);
    [DllImport("kernel32.dll")]
    private static extern bool CloseHandle(IntPtr handle);

    public static Process Start(string executable, string arguments, string directory)
    {
        const uint UserScope = 2, NormalUser = 0x20000, OpenLevel = 1, NoConsole = 0x08000000;
        if (!SaferCreateLevel(UserScope, NormalUser, OpenLevel, out IntPtr level, IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot create the normal-user SAFER level.");
        IntPtr token = IntPtr.Zero;
        try
        {
            if (!SaferComputeTokenFromLevel(level, IntPtr.Zero, out token, 0, IntPtr.Zero))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot restrict the drag peer token.");
            var medium = new SecurityIdentifier("S-1-16-8192");
            byte[] sid = new byte[medium.BinaryLength];
            medium.GetBinaryForm(sid, 0);
            int labelSize = Marshal.SizeOf<MandatoryLabel>();
            IntPtr buffer = Marshal.AllocHGlobal(labelSize + sid.Length);
            try
            {
                IntPtr sidAddress = IntPtr.Add(buffer, labelSize);
                Marshal.Copy(sid, 0, sidAddress, sid.Length);
                Marshal.StructureToPtr(new MandatoryLabel { Sid = sidAddress, Attributes = 0x20 }, buffer, false);
                if (!SetTokenInformation(token, 25, buffer, labelSize + sid.Length))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot set medium integrity for the drag peer.");
            }
            finally { Marshal.FreeHGlobal(buffer); }
            var startup = new StartupInfo { Size = Marshal.SizeOf<StartupInfo>(), Desktop = @"winsta0\default" };
            var command = new StringBuilder("\"" + executable + "\" " + arguments);
            if (!CreateProcessAsUserW(token, executable, command, IntPtr.Zero, IntPtr.Zero, false,
                NoConsole, IntPtr.Zero, directory, ref startup, out ProcessInfo started))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot start the normal-user drag peer.");
            Process managed = null;
            try
            {
                managed = Process.GetProcessById((int)started.ProcessId);
                // Retain a handle before releasing the creation handle. Existing
                // peer ownership and cleanup checks can then use Process directly.
                _ = managed.Handle;
                return managed;
            }
            catch { managed?.Dispose(); TerminateProcess(started.Process, 1); throw; }
            finally { CloseHandle(started.Thread); CloseHandle(started.Process); }
        }
        finally
        {
            if (token != IntPtr.Zero) CloseHandle(token);
            SaferCloseLevel(level);
        }
    }
}
