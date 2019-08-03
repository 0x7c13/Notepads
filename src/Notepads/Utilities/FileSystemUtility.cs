
namespace Notepads.Utilities
{
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.ApplicationModel.Resources;
    using Windows.Storage;
    using Windows.Storage.FileProperties;
    using Windows.Storage.Provider;

    public class TextFile
    {
        public TextFile(string content, Encoding encoding, LineEnding lineEnding)
        {
            Content = content;
            Encoding = encoding;
            LineEnding = lineEnding;
        }

        public string Content { get; }
        public Encoding Encoding { get; }
        public LineEnding LineEnding { get; }
    }

    public static class FileSystemUtility
    {
        private static readonly ResourceLoader ResourceLoader = ResourceLoader.GetForCurrentView();

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
            var path = GetAbsolutePathFromCommondLine(dir, args);

            if (string.IsNullOrEmpty(path))
            {
                return null;
            }

            try
            {
                return await StorageFile.GetFileFromPathAsync(path);
            }
            catch (Exception)
            {
                // ignore
            }

            return null;
        }

        public static string GetAbsolutePathFromCommondLine(string dir, string args)
        {
            if (string.IsNullOrEmpty(args)) return null;

            string path = args;

            if (path.StartsWith("\"") && path.EndsWith("\"") && path.Length > 2)
            {
                path = path.Substring(1, args.Length - 2);
            }

            if (FileSystemUtility.IsFullPath(path))
            {
                return path;
            }

            if (path.StartsWith(".\\"))
            {
                path = dir + Path.DirectorySeparatorChar + path.Substring(2, path.Length - 2);
            }
            else if (path.StartsWith("..\\"))
            {
                path = FileSystemUtility.GetAbsolutePath(dir, path);
            }
            else
            {
                path = dir + Path.DirectorySeparatorChar + path;
            }

            return path;
        }

        public static async Task<BasicProperties> GetFileProperties(StorageFile file)
        {
            return await file.GetBasicPropertiesAsync();
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

        public static async Task<TextFile> ReadFile(StorageFile file)
        {
            var fileProperties = await file.GetBasicPropertiesAsync();

            if (fileProperties.Size > 1000 * 1024) // 1MB
            {
                throw new Exception(ResourceLoader.GetString("ErrorMessage_NotepadsFileSizeLimit"));
            }

            Encoding.RegisterProvider(System.Text.CodePagesEncodingProvider.Instance);

            string text;
            Encoding encoding;
            var bom = new byte[4];

            using (var inputStream = await file.OpenReadAsync())
            using (var classicStream = inputStream.AsStreamForRead())
            {
                classicStream.Read(bom, 0, 4);
            }

            using (var inputStream = await file.OpenReadAsync())
            using (var classicStream = inputStream.AsStreamForRead())
            {
                var reader = HasBom(bom) ? new StreamReader(classicStream) : new StreamReader(classicStream, EditorSettingsService.EditorDefaultDecoding);
                reader.Peek();
                encoding = reader.CurrentEncoding;
                text = reader.ReadToEnd();
                reader.Close();
            }

            encoding = FixUtf8Bom(encoding, bom);
            return new TextFile(text, encoding, LineEndingUtility.GetLineEndingTypeFromText(text));
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
                    Analytics.TrackEvent("CachedFileManager_CompleteUpdatesAsync_Failed", new Dictionary<string, string>() {
                        {
                            "FileUpdateStatus", nameof(status)
                        }
                    });
                    throw new Exception($"FileUpdateStatus: {nameof(status)}");
                }
            }
        }

        public static async Task<StorageFolder> GetOrCreateAppFolder(string folderName)
        {
            StorageFolder localFolder = ApplicationData.Current.LocalFolder;
            return await localFolder.CreateFolderAsync(folderName, CreationCollisionOption.OpenIfExists);
        }

        public static async Task<StorageFile> CreateFile(StorageFolder folder, string fileName)
        {
            return await folder.CreateFileAsync(fileName);
        }
    }
}
