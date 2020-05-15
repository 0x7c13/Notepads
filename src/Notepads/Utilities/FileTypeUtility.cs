﻿namespace Notepads.Utilities
{
    using System;
    using System.Linq;

    public enum FileType
    {
        Unknown = 0,
        TextFile,
        MarkdownFile,
        JsonFile,
    }

    public static class FileTypeUtility
    {
        public static string GetFileExtension(string filename)
        {
            if (string.IsNullOrEmpty(filename) || !filename.Contains("."))
            {
                return string.Empty;
            }

            return filename.Substring(filename.LastIndexOf(".", StringComparison.Ordinal));
        }

        public static FileType GetFileTypeByFileName(string filename)
        {
            if (string.IsNullOrEmpty(filename) || !filename.Contains("."))
            {
                return FileType.Unknown;
            }

            return GetFileTypeByFileExtension(filename.Split(".").Last());
        }

        public static FileType GetFileTypeByFileExtension(string extension)
        {
            if (string.IsNullOrEmpty(extension))
            {
                return FileType.Unknown;
            }

            var ext = extension.ToLower();

            if (ext == "txt")
            {
                return FileType.TextFile;
            }

            if (ext == "md" || ext == "markdown")
            {
                return FileType.MarkdownFile;
            }

            if (ext == "json")
            {
                return FileType.JsonFile;
            }

            return FileType.Unknown;
        }

        public static bool IsPreviewSupported(FileType fileType)
        {
            if (fileType == FileType.MarkdownFile)
            {
                return true;
            }

            return false;
        }

        public static string GetDisplayText(FileType fileType)
        {
            switch (fileType)
            {
                case FileType.Unknown:
                    return string.Empty;
                case FileType.TextFile:
                    return "TXT";
                case FileType.MarkdownFile:
                    return "Markdown";
                case FileType.JsonFile:
                    return "JSON";
                default:
                    return string.Empty;
            }
        }
    }
}