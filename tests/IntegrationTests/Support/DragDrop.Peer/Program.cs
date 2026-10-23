// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Security.Principal;
using System.Windows.Forms;

namespace NotepadsDragDropPeer;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        PeerChannel? channel = null;
        try
        {
            if (args.Length != 1)
            {
                throw new ArgumentException("Expected one absolute path to an existing empty artifacts channel.");
            }

            channel = new PeerChannel(args[0]);
            using WindowsIdentity identity = WindowsIdentity.GetCurrent();
            bool elevated = new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
            channel.WriteAtomic("identity.txt", $"User={identity.Name}\nElevated={elevated}\n");
            if (Environment.GetEnvironmentVariable("RUNNER_ENVIRONMENT") == "github-hosted" && elevated)
            {
                throw new InvalidOperationException("The hosted drag peer must run with normal-user rights.");
            }
            Application.SetUnhandledExceptionMode(UnhandledExceptionMode.ThrowException);
            Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            using PeerForm form = new(channel);
            Application.Run(form);
            return 0;
        }
        catch (Exception exception)
        {
            channel?.WriteAtomic("fatal.txt", exception.ToString());
            return 1;
        }
        finally
        {
            channel?.Dispose();
        }
    }
}
