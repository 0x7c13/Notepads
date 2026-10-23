// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

using System;
using System.IO;
using Tool;

if (args.Length == 0 || !string.Equals(args[0], "Interface", StringComparison.OrdinalIgnoreCase) || args.Length > 2)
{
    Console.Error.WriteLine("Usage: Tool Interface [native source directory]");
    return 1;
}

DirectoryInfo nativeDirectory = null;
if (args.Length == 2)
{
    nativeDirectory = new DirectoryInfo(Path.GetFullPath(args[1]));
}
else
{
    for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory != null; directory = directory.Parent)
    {
        if (File.Exists(Path.Combine(directory.FullName, "WinUIEditor", "WinUIEditor.iface")))
        {
            nativeDirectory = directory;
            break;
        }
    }
}

if (nativeDirectory == null || !File.Exists(Path.Combine(nativeDirectory.FullName, "WinUIEditor", "WinUIEditor.iface")))
{
    Console.Error.WriteLine("Could not locate native sources. Supply the directory containing WinUIEditor and scintilla.");
    return 1;
}

Console.WriteLine("Generating the editor interface...");
await new InterfaceGeneratorTool().RunAsync(nativeDirectory.FullName);
Console.WriteLine("Editor interface generated.");
return 0;
