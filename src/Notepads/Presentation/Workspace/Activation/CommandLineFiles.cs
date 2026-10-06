// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

namespace Notepads.Presentation.Workspace.Activation;

public static class CommandLineFiles
{
    private const string WslRootPath = "\\\\wsl$\\";
    public static bool IsFullPath(string path)
    {
        return !String.IsNullOrWhiteSpace(path)
               && path.IndexOfAny(System.IO.Path.GetInvalidPathChars().ToArray()) == -1
               && Path.IsPathRooted(path)
               && !Path.GetPathRoot(path).Equals(Path.DirectorySeparatorChar.ToString(), StringComparison.Ordinal);
    }

    public static String GetAbsolutePath(String basePath, String path)
    {
        String finalPath;
        if (!Path.IsPathRooted(path) || "\\".Equals(Path.GetPathRoot(path)))
        {
            if (path.StartsWith(Path.DirectorySeparatorChar.ToString()))
            {
                finalPath = Path.Combine(Path.GetPathRoot(basePath), path.TrimStart(Path.DirectorySeparatorChar));
            }
            else
            {
                finalPath = Path.Combine(basePath, path);
            }
        }
        else
        {
            finalPath = path;
        }

        // Resolves any internal "..\" to get the true full path.
        return Path.GetFullPath(finalPath);
    }

    public static async Task<StorageFile> OpenFileFromCommandLineAsync(string dir, string args, string applicationName)
    {
        string path = null;

        try
        {
            args = ReplaceEnvironmentVariables(args);
            path = GetAbsolutePathFromCommandLine(dir, args, applicationName);
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[{nameof(CommandLineFiles)}] Failed to parse command line: {args} with Exception: {ex}");
        }

        if (string.IsNullOrEmpty(path))
        {
            return null;
        }

        LoggingService.LogInfo($"[{nameof(CommandLineFiles)}] OpenFileFromCommandLine: {path}");

        return await FileStorage.GetFileAsync(path);
    }

    private static string ReplaceEnvironmentVariables(string args)
    {
        if (args.Contains("%homepath%", StringComparison.OrdinalIgnoreCase))
        {
            args = args.Replace("%homepath%",
                Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
                StringComparison.OrdinalIgnoreCase);
        }

        if (args.Contains("%localappdata%", StringComparison.OrdinalIgnoreCase))
        {
            args = args.Replace("%localappdata%",
                UserDataPaths.GetDefault().LocalAppData,
                StringComparison.OrdinalIgnoreCase);
        }

        if (args.Contains("%temp%", StringComparison.OrdinalIgnoreCase))
        {
            args = args.Replace("%temp%",
                (string)Microsoft.Win32.Registry.GetValue(@"HKEY_CURRENT_USER\Environment",
                "TEMP",
                Environment.GetEnvironmentVariable("temp")),
                StringComparison.OrdinalIgnoreCase);
        }

        if (args.Contains("%tmp%", StringComparison.OrdinalIgnoreCase))
        {
            args = args.Replace("%tmp%",
                (string)Microsoft.Win32.Registry.GetValue(@"HKEY_CURRENT_USER\Environment",
                "TEMP",
                Environment.GetEnvironmentVariable("tmp")),
                StringComparison.OrdinalIgnoreCase);
        }

        return Environment.ExpandEnvironmentVariables(args);
    }

    private static string GetAbsolutePathFromCommandLine(string dir, string args, string appName)
    {
        if (string.IsNullOrEmpty(args)) return null;

        args = args.Trim();

        args = RemoveExecutableNameOrPathFromCommandLineArgs(args, appName);

        if (string.IsNullOrEmpty(args))
        {
            return null;
        }

        string path = args;

        // Get first quoted string if any
        if (path.StartsWith("\"") && path.Length > 1)
        {
            var index = path.IndexOf('\"', 1);
            if (index == -1) return null;
            path = args.Substring(1, index - 1);
        }

        if (dir.StartsWith(WslRootPath))
        {
            if (path.StartsWith('/'))
            {
                var distroRootPath = dir.Substring(0, dir.IndexOf('\\', WslRootPath.Length) + 1);
                var fullPath = distroRootPath + path.Trim('/').Replace('/', Path.DirectorySeparatorChar);
                if (IsFullPath(fullPath)) return fullPath;
            }
        }

        // Replace all forward slash with platform supported directory separator
        path = path.Trim('/').Replace('/', Path.DirectorySeparatorChar);

        if (IsFullPath(path))
        {
            return path;
        }

        if (path.StartsWith(".\\"))
        {
            path = dir + Path.DirectorySeparatorChar + path.Substring(2, path.Length - 2);
        }
        else if (path.StartsWith("..\\"))
        {
            path = GetAbsolutePath(dir, path);
        }
        else
        {
            path = dir + Path.DirectorySeparatorChar + path;
        }

        return path;
    }

    private static string RemoveExecutableNameOrPathFromCommandLineArgs(string args, string appName)
    {
        if (!args.StartsWith('\"'))
        {
            // From Windows Command Line
            // notepads <file> ...
            // notepads.exe <file>

            if (args.StartsWith($"{appName}-Dev.exe",
                StringComparison.OrdinalIgnoreCase))
            {
                args = args.Substring($"{appName}-Dev.exe".Length);
            }

            if (args.StartsWith($"{appName}.exe",
                StringComparison.OrdinalIgnoreCase))
            {
                args = args.Substring($"{appName}.exe".Length);
            }

            if (args.StartsWith($"{appName}-Dev",
                StringComparison.OrdinalIgnoreCase))
            {
                args = args.Substring($"{appName}-Dev".Length);
            }

            if (args.StartsWith(appName,
                StringComparison.OrdinalIgnoreCase))
            {
                args = args.Substring(appName.Length);
            }
        }
        else if (args.StartsWith('\"') && args.Length > 1)
        {
            // From PowerShell or run
            // "notepads" <file>
            // "notepads.exe" <file>
            // "<app-install-path><app-name>.exe"  <file> ...
            var index = args.IndexOf('\"', 1);
            if (index == -1) return null;
            if (args.Length == index + 1) return null;
            args = args.Substring(index + 1);
        }
        else
        {
            return null;
        }

        args = args.Trim();
        return args;
    }

}
