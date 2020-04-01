namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Models;
    using Notepads.Services;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.Storage.AccessCache;
    using Windows.Storage.FileProperties;
    using Windows.Storage.Provider;
    using UtfUnknown;

    public static class FileSystemUtility
    {
        private static readonly ResourceLoader ResourceLoader = ResourceLoader.GetForCurrentView();

        private static readonly string wslRootPath = "\\\\wsl$\\";

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
                    finalPath = Path.Combine(Path.GetPathRoot(basePath), path.TrimStart(Path.DirectorySeparatorChar));
                else
                    finalPath = Path.Combine(basePath, path);
            }
            else
                finalPath = path;
            // Resolves any internal "..\" to get the true full path.
            return Path.GetFullPath(finalPath);
        }

        public static async Task<StorageFile> OpenFileFromCommandLine(string dir, string args)
        {
            string path = null;

            try
            {
                path = GetAbsolutePathFromCommandLine(dir, args, App.ApplicationName);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to parse command line: {args} with Exception: {ex}");
            }

            if (string.IsNullOrEmpty(path))
            {
                return null;
            }

            LoggingService.LogInfo($"OpenFileFromCommandLine: {path}");

            return await GetFile(path);
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

            if (dir.StartsWith(wslRootPath))
            {
                if (path.StartsWith('/'))
                {
                    var distroRootPath = dir.Substring(0, dir.IndexOf('\\', wslRootPath.Length) + 1);
                    var fullPath = distroRootPath + path.Trim('/').Replace('/', Path.DirectorySeparatorChar);
                    if (IsFullPath(fullPath)) return fullPath;
                }
                path = path.Trim('/').Replace('/', Path.DirectorySeparatorChar);
            }

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

                if (args.StartsWith($"{appName}.exe",
                    StringComparison.OrdinalIgnoreCase))
                {
                    args = args.Substring($"{appName}.exe".Length);
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

        public static async Task<BasicProperties> GetFileProperties(StorageFile file)
        {
            return await file.GetBasicPropertiesAsync();
        }

        public static async Task<long> GetDateModified(StorageFile file)
        {
            var properties = await GetFileProperties(file);
            var dateModified = properties.DateModified;
            return dateModified.ToFileTime();
        }

        public static bool IsFileReadOnly(StorageFile file)
        {
            return (file.Attributes & Windows.Storage.FileAttributes.ReadOnly) != 0;
        }

        public static async Task<bool> FileIsWritable(StorageFile file)
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

        public static async Task<StorageFile> GetFile(string filePath)
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

        public static async Task<TextFile> ReadFile(string filePath, bool ignoreFileSizeLimit, Encoding encoding)
        {
            StorageFile file = await GetFile(filePath);
            return file == null ? null : await ReadFile(file, ignoreFileSizeLimit, encoding);
        }

        public static async Task<TextFile> ReadFile(StorageFile file, bool ignoreFileSizeLimit, Encoding encoding = null)
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
                    if (EditorSettingsService.EditorDefaultDecoding == null)
                    {
                        var success = TryGuessEncoding(stream, out var autoEncoding);
                        stream.Position = 0; // Reset stream position
                        reader = success ? 
                            new StreamReader(stream, autoEncoding) : 
                            new StreamReader(stream, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true));
                    }
                    else
                    {
                        reader = new StreamReader(stream, EditorSettingsService.EditorDefaultDecoding);   
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

        // Will throw if not succeeded, exception should be caught and handled by caller
        public static async Task WriteToFile(string text, Encoding encoding, StorageFile file)
        {
            bool usedDeferUpdates = true;

            try
            {
                // Prevent updates to the remote version of the file until we 
                // finish making changes and call CompleteUpdatesAsync.
                CachedFileManager.DeferUpdates(file);
            }
            catch (Exception)
            {
                // If DeferUpdates fails, just ignore it and try to save the file anyway
                usedDeferUpdates = false;
            }

            // Write to file
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);

            using (var stream = await file.OpenStreamForWriteAsync())
            using (var writer = new StreamWriter(stream, encoding))
            {
                stream.Position = 0;
                writer.Write(text);
                writer.Flush();
                // Truncate
                stream.SetLength(stream.Position);
            }

            if (usedDeferUpdates)
            {
                // Let Windows know that we're finished changing the file so the 
                // other app can update the remote version of the file.
                FileUpdateStatus status = await CachedFileManager.CompleteUpdatesAsync(file);
                if (status != FileUpdateStatus.Complete)
                {
                    // Track FileUpdateStatus here to better understand the failed scenarios
                    // File name, path and content are not included to respect/protect user privacy 
                    Analytics.TrackEvent("CachedFileManager_CompleteUpdatesAsync_Failed", new Dictionary<string, string>() 
                    {
                        { "FileUpdateStatus", nameof(status) }
                    });
                    throw new Exception($"Failed to invoke [CompleteUpdatesAsync], FileUpdateStatus: {nameof(status)}");
                }
            }
        }

        internal static async Task DeleteFile(string filePath, StorageDeleteOption deleteOption = StorageDeleteOption.PermanentDelete)
        {
            try
            {
                var file = await GetFile(filePath);
                if (file != null)
                {
                    await file.DeleteAsync(deleteOption);
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to delete file: {filePath}, Exception: {ex.Message}");
            }
        }

        public static async Task<StorageFolder> GetOrCreateAppFolder(string folderName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            return await localFolder.CreateFolderAsync(folderName, CreationCollisionOption.OpenIfExists);
        }

        public static async Task<StorageFile> CreateFile(StorageFolder folder, string fileName, CreationCollisionOption option = CreationCollisionOption.ReplaceExisting)
        {
            return await folder.CreateFileAsync(fileName, option);
        }

        public static async Task<bool> FileExists(StorageFile file)
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
                LoggingService.LogError($"Failed to check if file [{file.Path}] exists: {ex.Message}", consoleOnly: true);
                return true;
            }
        }

        public static async Task<StorageFile> GetFileFromFutureAccessList(string token)
        {
            try
            {
                if (StorageApplicationPermissions.FutureAccessList.ContainsItem(token))
                {
                    return await StorageApplicationPermissions.FutureAccessList.GetFileAsync(token);
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to get file from future access list: {ex.Message}");
            }
            return null;
        }

        public static async Task<bool> TryAddOrReplaceTokenInFutureAccessList(string token, StorageFile file)
        {
            try
            {
                if (await FileExists(file))
                {
                    StorageApplicationPermissions.FutureAccessList.AddOrReplace(token, file);
                    return true;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to add file [{file.Path}] to future access list: {ex.Message}");
                Analytics.TrackEvent("FailedToAddTokenInFutureAccessList", 
                    new Dictionary<string, string>()
                    {
                        { "ItemCount", GetFutureAccessListItemCount().ToString() },
                        { "Exception", ex.Message }
                    });
            }
            return false;
        }

        public static int GetFutureAccessListItemCount()
        {
            try
            {
                return StorageApplicationPermissions.FutureAccessList.Entries.Count;
            }
            catch
            {
                return -1;
            }
        }
    }
}