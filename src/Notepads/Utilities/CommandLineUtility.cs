namespace Notepads.Utilities
{
    using System;
    using System.IO;
    using System.Runtime.InteropServices;
    using System.Threading.Tasks;
    using Microsoft.Win32;
    using Notepads.Services;
    using Windows.Storage;
    using Windows.System;

    public static class CommandLineUtility
    {
        private static bool _hasSetEnvironmentVariables = false;

        public static bool IsFullPath(string path)
        {
            return !string.IsNullOrWhiteSpace(path)
                   && path.IndexOfAny(Path.GetInvalidPathChars()) == -1
                   && Path.IsPathRooted(path)
                   && !Path.GetPathRoot(path).Equals(Path.DirectorySeparatorChar.ToString(), StringComparison.Ordinal);
        }

        public static string GetAbsolutePath(string basePath, string path)
        {
            // Resolves any internal "..\" to get the true full path.
            return Path.GetFullPath(
                Path.GetPathRoot(path).Equals(Path.DirectorySeparatorChar.ToString(), StringComparison.Ordinal)
                ? Path.Combine(Path.GetPathRoot(basePath), path.TrimStart(Path.DirectorySeparatorChar))
                : Path.Combine(basePath, path));
        }

        public static async Task<IStorageFile> OpenFileFromCommandLine(string dir, string args)
        {
            SetEnvironmentVariables();

            string path = null;
            try
            {
                path = GetAbsolutePathFromCommandLine(dir, args);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(FileSystemUtility)}] Failed to parse command line: {args} with Exception: {ex}");
            }

            if (string.IsNullOrEmpty(path))
            {
                return null;
            }

            LoggingService.LogInfo($"[{nameof(FileSystemUtility)}] OpenFileFromCommandLine: {path}");

            var result = await FileSystemUtility.GetFile(path);
            if (result.Item2 != null)
            {
                DispatcherQueue.GetForCurrentThread().TryEnqueue(
                    () => NotificationCenter.Instance.PostNotification(result.Item2.Message, 1500)
                );
            }
            return result.Item1;
        }

        private static void SetEnvironmentVariables()
        {
            if (_hasSetEnvironmentVariables) return;

            Environment.SetEnvironmentVariable(
                "homepath",
                UserDataPaths.GetDefault().Profile);

            Environment.SetEnvironmentVariable(
                "localappdata",
                UserDataPaths.GetDefault().LocalAppData);

            Environment.SetEnvironmentVariable(
                "temp",
                (string)Registry.GetValue(
                    @"HKEY_CURRENT_USER\Environment",
                    "TEMP",
                    Environment.GetEnvironmentVariable("temp")));

            Environment.SetEnvironmentVariable(
                "tmp",
                (string)Registry.GetValue(
                    @"HKEY_CURRENT_USER\Environment",
                    "TEMP",
                    Environment.GetEnvironmentVariable("tmp")));

            _hasSetEnvironmentVariables = true;
        }

        internal static string GetAbsolutePathFromCommandLine(string dir, string args)
        {
            if (string.IsNullOrEmpty(args)) return null;

            var path = Environment.ExpandEnvironmentVariables(
                RemoveExecutableNameOrPathFromCommandLineArgs(args.Trim()));

            if (string.IsNullOrEmpty(path))
            {
                return null;
            }

            // Replace all forward slash with platform supported directory separator
            path = path.Replace('/', Path.DirectorySeparatorChar);

            if (IsFullPath(path))
            {
                return path;
            }

            path = GetAbsolutePath(dir, path);

            return path;
        }

        private static string RemoveExecutableNameOrPathFromCommandLineArgs(string args)
        {
            var argv = CommandLineToArgvW(args, out int argc);
            if (argc <= 1) return null;

            var argsBuilder = new string[argc - 1];
            try
            {
                for (var i = 0; i < argsBuilder.Length; i++)
                {
                    argsBuilder[i] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(argv, (i + 1) * IntPtr.Size));
                }
            }
            finally
            {
                Marshal.FreeHGlobal(argv);
            }
            return string.Join(' ', argsBuilder).Trim();
        }

        #region Win32 and COM APIs

        [DllImport("api-ms-win-shcore-obsolete-l1-1-0.dll", CharSet = CharSet.Auto,
        CallingConvention = CallingConvention.StdCall,
        SetLastError = true)]
        private unsafe static extern IntPtr CommandLineToArgvW(
            [MarshalAs(UnmanagedType.LPWStr)] string lpCmdLine,
            out int pNumArgs
        );

        #endregion
    }
}