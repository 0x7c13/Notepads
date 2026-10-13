// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Features.Documents.FileTypes;

public enum FileType
{
    Unknown = 0,
    TextFile,
    MarkdownFile,
}

public static class FileTypeUtility
{
    public static string GetFileExtension(string filename)
    {
        var dot = filename?.LastIndexOf('.') ?? -1;
        if (dot < 0)
        {
            return string.Empty;
        }

        return filename[dot..];
    }

    public static FileType GetFileTypeByFileName(ReadOnlySpan<char> filename)
    {
        var dot = filename.LastIndexOf('.');
        if (dot < 0)
        {
            return FileType.Unknown;
        }

        return GetFileTypeByFileExtension(filename[(dot + 1)..]);
    }

    public static FileType GetFileTypeByFileExtension(ReadOnlySpan<char> extension)
    {
        if (extension.Equals("txt", StringComparison.OrdinalIgnoreCase))
        {
            return FileType.TextFile;
        }

        if (extension.Equals("md", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals("markdown", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals("mkd", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals("mdwn", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals("mdown", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals("markdn", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals("mdtxt", StringComparison.OrdinalIgnoreCase))
        {
            return FileType.MarkdownFile;
        }

        return FileType.Unknown;
    }

    public static bool IsPreviewSupported(FileType fileType)
    {
        // Preview rendering is suspended until the V2 renderers are implemented.
        return false;
    }

    public static string GetDisplayText(FileType fileType)
    {
        return fileType switch
        {
            FileType.Unknown => string.Empty,
            FileType.TextFile => "TXT",
            FileType.MarkdownFile => "Markdown",
            _ => string.Empty,
        };
    }
}
