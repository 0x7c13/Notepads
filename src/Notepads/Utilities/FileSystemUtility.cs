namespace Notepads.Utilities
{
    using Microsoft.AppCenter.Analytics;
    using Notepads.Models;
    using Notepads.Services;
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Linq;
    using System.Text;
    using System.Text.RegularExpressions;
    using System.Threading.Tasks;
    using UtfUnknown;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.Storage.FileProperties;
    using Windows.Storage.Provider;

    public enum InvalidFilenameError
    {
        None = 0,
        EmptyOrAllWhitespace,
        ContainsLeadingSpaces,
        ContainsTrailingSpaces,
        ContainsInvalidCharacters,
        InvalidOrNotAllowed,
        TooLong,
    }

    public static class FileSystemUtility
    {
        // Retriable FileIO errors
        private const Int32 ERROR_ACCESS_DENIED = unchecked((Int32)0x80070005);
        private const Int32 ERROR_SHARING_VIOLATION = unchecked((Int32)0x80070020);
        private const Int32 ERROR_UNABLE_TO_REMOVE_REPLACED = unchecked((Int32)0x80070497);
        private const Int32 ERROR_FAIL = unchecked((Int32)0x80004005);

        private static readonly ResourceLoader ResourceLoader = ResourceLoader.GetForCurrentView();

        private const string WslRootPath = "\\\\wsl$\\";

        // https://stackoverflow.com/questions/62771/how-do-i-check-if-a-given-string-is-a-legal-valid-file-name-under-windows
        private static readonly Regex ValidWindowsFileNames = new Regex(@"^(?!(?:PRN|AUX|CLOCK\$|NUL|CON|COM\d|LPT\d)(?:\..+)?$)[^\x00-\x1F\xA5\\?*:\"";|\/<>]+(?<![\s.])$", RegexOptions.IgnoreCase);

        public static bool IsFilenameValid(string filename, out InvalidFilenameError error)
        {
            if (filename.Length > 255)
            {
                error = InvalidFilenameError.TooLong;
                return false;
            }

            if (string.IsNullOrWhiteSpace(filename))
            {
                error = InvalidFilenameError.EmptyOrAllWhitespace;
                return false;
            }

            // Although shell supports file with leading spaces, explorer and file picker does not
            // So we treat it as invalid file name as well
            if (filename.StartsWith(" "))
            {
                error = InvalidFilenameError.ContainsLeadingSpaces;
                return false;
            }

            if (filename.EndsWith(" "))
            {
                error = InvalidFilenameError.ContainsTrailingSpaces;
                return false;
            }

            var illegalChars = Path.GetInvalidFileNameChars();
            if (filename.Any(c => illegalChars.Contains(c)))
            {
                error = InvalidFilenameError.ContainsInvalidCharacters;
                return false;
            }

            if (filename.EndsWith(".") || !ValidWindowsFileNames.IsMatch(filename))
            {
                error = InvalidFilenameError.InvalidOrNotAllowed;
                return false;
            }

            error = InvalidFilenameError.None;
            return true;
        }

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

        public static async Task<StorageFile> OpenFileFromCommandLineAsync(string dir, string args)
        {
            string path = null;

            try
            {
                args = ReplaceEnvironmentVariables(args);
                path = GetAbsolutePathFromCommandLine(dir, args, App.ApplicationName);
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

            return await GetFileAsync(path);
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

        public static string GetAbsolutePathFromCommandLine(string dir, string args, string appName)
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

        public static async Task<BasicProperties> GetFilePropertiesAsync(StorageFile file)
        {
            return await file.GetBasicPropertiesAsync();
        }

        public static async Task<long> GetDateModifiedAsync(StorageFile file)
        {
            var properties = await GetFilePropertiesAsync(file);
            var dateModified = properties.DateModified;
            return dateModified.ToFileTime();
        }

        public static bool IsFileReadOnly(StorageFile file)
        {
            return (file.Attributes & Windows.Storage.FileAttributes.ReadOnly) != 0;
        }

        public static async Task<bool> IsFileWritableAsync(StorageFile file)
        {
            try
            {
                using (var stream = await file.OpenStreamForWriteAsync()) { }
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }

        public static async Task<StorageFile> GetFileAsync(string filePath)
        {
            try
            {
                return await StorageFile.GetFileFromPathAsync(filePath);
            }
            catch
            {
                return null;
            }
        }

        public static async Task<TextFile> ReadFileAsync(string filePath, bool ignoreFileSizeLimit, Encoding encoding)
        {
            StorageFile file = await GetFileAsync(filePath);
            return file == null ? null : await ReadFileAsync(file, ignoreFileSizeLimit, encoding);
        }

        public static async Task<TextFile> ReadFileAsync(StorageFile file, bool ignoreFileSizeLimit, Encoding encoding = null)
        {
            var fileProperties = await file.GetBasicPropertiesAsync();

            if (!ignoreFileSizeLimit && fileProperties.Size > 1000 * 1024)
            {
                throw new Exception(ResourceLoader.GetString("ErrorMessage_NotepadsFileSizeLimit"));
            }

            Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);

            string text;
            var bom = new byte[4];

            using (var inputStream = await file.OpenReadAsync())
            using (var stream = inputStream.AsStreamForRead())
            {
                stream.Read(bom, 0, 4); // Read BOM values
                stream.Position = 0; // Reset stream position

                var reader = CreateStreamReader(stream, bom, encoding);

                string PeekAndRead()
                {
                    if (encoding == null)
                    {
                        reader.Peek();
                        encoding = reader.CurrentEncoding;
                    }
                    var str = reader.ReadToEnd();
                    reader.Close();
                    return str;
                }

                try
                {
                    text = PeekAndRead();
                }
                catch (DecoderFallbackException)
                {
                    stream.Position = 0; // Reset stream position
                    encoding = GetFallBackEncoding();
                    reader = new StreamReader(stream, encoding);
                    text = PeekAndRead();
                }
            }

            encoding = FixUtf8Bom(encoding, bom);
            return new TextFile(text, encoding, LineEndingUtility.GetLineEndingTypeFromText(text), fileProperties.DateModified.ToFileTime());
        }

        private static Encoding GetFallBackEncoding()
        {
            if (EncodingUtility.TryGetSystemDefaultANSIEncoding(out var systemDefaultEncoding))
            {
                return systemDefaultEncoding;
            }
            else if (EncodingUtility.TryGetCurrentCultureANSIEncoding(out var currentCultureEncoding))
            {
                return currentCultureEncoding;
            }
            else
            {
                return new UTF8Encoding(false);
            }
        }

        private static StreamReader CreateStreamReader(Stream stream, byte[] bom, Encoding encoding = null)
        {
            StreamReader reader;
            if (encoding != null)
            {
                reader = new StreamReader(stream, encoding);
            }
            else
            {
                if (HasBom(bom))
                {
                    reader = new StreamReader(stream);
                }
                else // No BOM, need to guess or use default decoding set by user
                {
                    if (AppSettingsService.EditorDefaultDecoding == null)
                    {
                        var success = TryGuessEncoding(stream, out var autoEncoding);
                        stream.Position = 0; // Reset stream position
                        reader = success ?
                            new StreamReader(stream, autoEncoding) :
                            new StreamReader(stream, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true));
                    }
                    else
                    {
                        reader = new StreamReader(stream, AppSettingsService.EditorDefaultDecoding);
                    }
                }
            }
            return reader;
        }

        public static bool TryGuessEncoding(Stream stream, out Encoding encoding)
        {
            encoding = null;

            try
            {
                var result = CharsetDetector.DetectFromStream(stream);
                if (result.Detected?.Encoding != null) // Detected can be null
                {
                    encoding = AnalyzeAndGuessEncoding(result);
                    return true;
                }
                else if (stream.Length > 0) // We do not care about empty file
                {
                    Analytics.TrackEvent("UnableToDetectEncoding");
                }
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("TryGuessEncodingFailedWithException", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() },
                    { "Message", ex.Message }
                });
            }

            return false;
        }

        private static Encoding AnalyzeAndGuessEncoding(DetectionResult result)
        {
            Encoding encoding = result.Detected.Encoding;
            var confidence = result.Detected.Confidence;
            var foundBetterMatch = false;

            // Let's treat ASCII as UTF-8 for better accuracy
            if (EncodingUtility.Equals(encoding, Encoding.ASCII)) encoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

            // If confidence is above 80%, we should just use it
            if (confidence > 0.80f && result.Details.Count == 1) return encoding;

            // Try find a better match based on User's current Windows ANSI code page
            // Priority: UTF-8 > SystemDefaultANSIEncoding (Codepage: 0) > CurrentCultureANSIEncoding
            if (!(encoding is UTF8Encoding))
            {
                foreach (var detail in result.Details)
                {
                    if (detail.Confidence <= 0.5f)
                    {
                        continue;
                    }
                    if (detail.Encoding is UTF8Encoding)
                    {
                        foundBetterMatch = true;
                    }
                    else if (EncodingUtility.TryGetSystemDefaultANSIEncoding(out var systemDefaultEncoding)
                             && EncodingUtility.Equals(systemDefaultEncoding, detail.Encoding))
                    {
                        foundBetterMatch = true;
                    }
                    else if (EncodingUtility.TryGetCurrentCultureANSIEncoding(out var currentCultureEncoding)
                             && EncodingUtility.Equals(currentCultureEncoding, detail.Encoding))
                    {
                        foundBetterMatch = true;
                    }

                    if (foundBetterMatch)
                    {
                        encoding = detail.Encoding;
                        confidence = detail.Confidence;
                        break;
                    }
                }
            }

            // We should fall back to UTF-8 and give it a try if:
            // 1. Detected Encoding is not UTF-8
            // 2. Detected Encoding is not SystemDefaultANSIEncoding (Codepage: 0)
            // 3. Detected Encoding is not CurrentCultureANSIEncoding
            // 4. Confidence of detected Encoding is below 50%
            if (!foundBetterMatch && confidence < 0.5f)
            {
                encoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
            }

            return encoding;
        }

        private static bool HasBom(byte[] bom)
        {
            // Analyze the BOM
            if (bom[0] == 0x2b && bom[1] == 0x2f && bom[2] == 0x76) return true; // Encoding.UTF7
            if (bom[0] == 0xef && bom[1] == 0xbb && bom[2] == 0xbf) return true; // Encoding.UTF8
            if (bom[0] == 0xff && bom[1] == 0xfe) return true; // Encoding.Unicode
            if (bom[0] == 0xfe && bom[1] == 0xff) return true; // Encoding.BigEndianUnicode
            if (bom[0] == 0 && bom[1] == 0 && bom[2] == 0xfe && bom[3] == 0xff) return true; // Encoding.UTF32
            return false;
        }

        private static Encoding FixUtf8Bom(Encoding encoding, byte[] bom)
        {
            if (encoding is UTF8Encoding)
            {
                // UTF8 with BOM - UTF-8-BOM
                // UTF8 byte order mark is: 0xEF,0xBB,0xBF
                if (bom[0] == 0xEF && bom[1] == 0xBB && bom[2] == 0xBF)
                {
                    encoding = new UTF8Encoding(true);
                }
                // UTF8 no BOM
                else
                {
                    encoding = new UTF8Encoding(false);
                }
            }

            return encoding;
        }

        /// <summary>
        /// Save text to a file with requested encoding
        /// Exception will be thrown if not succeeded
        /// Exception should be caught and handled by caller
        /// </summary>
        public static async Task WriteTextToFileAsync(StorageFile file, string text, Encoding encoding)
        {
            if (IsFileReadOnly(file) || !await IsFileWritableAsync(file))
            {
                await ExecuteFileIOOperationWithRetries(file,
                    "FileSystemUtility_WriteTextToFileAsync_UsingPathIO",
                    async () =>
                    {
                        // For file(s) dragged into Notepads, they are read-only
                        // StorageFile API won't work on read-only files but can be written by Win32 PathIO API (exploit?)
                        // In case the file is actually read-only, WriteBytesAsync will throw UnauthorizedAccessException
                        var content = encoding.GetBytes(text);
                        var result = encoding.GetPreamble().Concat(content).ToArray();
                        await PathIO.WriteBytesAsync(file.Path, result);
                    });
            }
            else // Use StorageFile API to save 
            {
                await ExecuteFileIOOperationWithRetries(file,
                    "FileSystemUtility_WriteTextToFileAsync_UsingStreamWriter",
                    async () =>
                    {
                        using (var stream = await file.OpenStreamForWriteAsync())
                        using (var writer = new StreamWriter(stream, encoding))
                        {
                            stream.Position = 0;
                            await writer.WriteAsync(text);
                            await writer.FlushAsync();
                            stream.SetLength(stream.Position); // Truncate
                        }
                    });
            }
        }

        /// <summary>
        /// Save text to a file with UTF-8 encoding using FileIO API with retries for retriable errors
        /// </summary>
        public static async Task WriteTextToFileAsync(StorageFile storageFile, string text)
        {
            await ExecuteFileIOOperationWithRetries(storageFile,
                "FileSystemUtility_WriteTextToFileAsync_UsingFileIO",
                async () => await FileIO.WriteTextAsync(storageFile, text));
        }

        private static async Task ExecuteFileIOOperationWithRetries(StorageFile file,
            string operationName,
            Func<Task> action,
            int maxRetryAttempts = 7)
        {
            bool deferUpdatesUsed = true;
            int retryAttempts = 0;

            try
            {
                // Prevent updates to the remote version of the file until we
                // finish making changes and call CompleteUpdatesAsync.
                CachedFileManager.DeferUpdates(file);
            }
            catch (Exception)
            {
                // If DeferUpdates fails, just ignore it and try to save the file anyway
                deferUpdatesUsed = false;
            }

            HashSet<string> errorCodes = new HashSet<string>();

            try
            {
                while (retryAttempts < maxRetryAttempts)
                {
                    try
                    {
                        retryAttempts++;
                        await action.Invoke(); // Execute FileIO action
                        break;
                    }
                    catch (Exception ex) when ((ex.HResult == ERROR_ACCESS_DENIED) ||
                                               (ex.HResult == ERROR_SHARING_VIOLATION) ||
                                               (ex.HResult == ERROR_UNABLE_TO_REMOVE_REPLACED) ||
                                               (ex.HResult == ERROR_FAIL))
                    {
                        errorCodes.Add("0x" + Convert.ToString(ex.HResult, 16));
                        await Task.Delay(13); // Delay 13ms before retrying for all retriable errors
                    }
                }
            }
            finally
            {
                string fileUpdateStatus = string.Empty;

                if (deferUpdatesUsed)
                {
                    // Let Windows know that we're finished changing the file so the
                    // other app can update the remote version of the file.
                    FileUpdateStatus status = await CachedFileManager.CompleteUpdatesAsync(file);
                    fileUpdateStatus = status.ToString();
                }

                if (retryAttempts > 1) // Retry attempts were used
                {
                    // Track retry attempts to better understand the failed scenarios
                    // File name, path and content are not included to respect/protect user privacy
                    Analytics.TrackEvent(operationName, new Dictionary<string, string>()
                    {
                        { "RetryAttempts", (retryAttempts - 1).ToString() },
                        { "DeferUpdatesUsed" , deferUpdatesUsed.ToString() },
                        { "ErrorCodes", string.Join(", ", errorCodes) },
                        { "FileUpdateStatus", fileUpdateStatus }
                    });
                }
            }
        }

        public static async Task<StorageFile> GetOrCreateFileAsync(StorageFolder folder, string fileName)
        {
            try
            {
                return await folder.CreateFileAsync(fileName, CreationCollisionOption.OpenIfExists);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(FileSystemUtility)}] Failed to get or create file, Exception: {ex.Message}");
                Analytics.TrackEvent("GetOrCreateFileAsync_Failed", new Dictionary<string, string>()
                {
                    { "Exception", ex.ToString() },
                });
                throw; // Rethrow
            }
        }

        internal static async Task DeleteFileAsync(string filePath, StorageDeleteOption deleteOption = StorageDeleteOption.PermanentDelete)
        {
            try
            {
                var file = await GetFileAsync(filePath);
                if (file != null)
                {
                    await file.DeleteAsync(deleteOption);
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(FileSystemUtility)}] Failed to delete file: {filePath}, Exception: {ex.Message}");
            }
        }

        public static async Task<StorageFolder> GetOrCreateAppFolderAsync(string folderName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            return await localFolder.CreateFolderAsync(folderName, CreationCollisionOption.OpenIfExists);
        }

        public static async Task<StorageFile> CreateFileAsync(StorageFolder folder, string fileName, CreationCollisionOption option = CreationCollisionOption.ReplaceExisting)
        {
            return await folder.CreateFileAsync(fileName, option);
        }

        public static async Task<bool> FileExistsAsync(StorageFile file)
        {
            try
            {
                using (var stream = await file.OpenStreamForReadAsync()) { }
                return true;
            }
            catch (FileNotFoundException)
            {
                return false;
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(FileSystemUtility)}] Failed to check if file [{file.Path}] exists: {ex.Message}", consoleOnly: true);
                return true;
            }
        }
    }
}