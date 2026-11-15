// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;

namespace Notepads.Presentation.Controls.Markdown;

internal static class MarkdownDestination
{
    internal static bool TryParse(string destination, bool wiki, bool allowMailto,
        out string localPath, out Uri remoteUri, out string fragment)
    {
        localPath = null;
        remoteUri = null;
        fragment = null;
        try
        {
            if (string.IsNullOrWhiteSpace(destination)) return false;
            var nativePath = wiki ? destination : destination.TrimStart();
            if (IsDevicePath(nativePath)) return false;

            // Native drive and UNC paths must keep Markdown query/fragment and
            // percent-escape semantics rather than URI's implicit-file semantics.
            bool drivePath = nativePath.Length >= 3 && char.IsAsciiLetter(nativePath[0]) &&
                nativePath[1] == ':' && nativePath[2] is '\\' or '/';
            bool networkPath = nativePath.StartsWith("\\\\", StringComparison.Ordinal) ||
                nativePath.StartsWith("//", StringComparison.Ordinal);
            if (!wiki && (drivePath || networkPath)) destination = nativePath;
            Uri uri = null;
            if (!wiki && !drivePath && !networkPath && Uri.TryCreate(destination, UriKind.Absolute, out uri))
            {
                if (uri.Scheme is "http" or "https" || (allowMailto && uri.Scheme == "mailto"))
                {
                    remoteUri = uri;
                    return true;
                }
                if (!uri.IsFile) return false;
            }

            int hash = destination.IndexOf('#');
            fragment = hash < 0 ? null : wiki ? destination[(hash + 1)..].Trim() : destination[hash..];
            string path = hash < 0 ? destination : destination[..hash];
            if (wiki)
            {
                path = path.TrimEnd();
                if (fragment?.Length == 0) fragment = null;
            }
            else
            {
                // LocalPath decodes an explicit file URI exactly once.
                path = uri?.IsFile == true ? uri.LocalPath : Uri.UnescapeDataString(path.Split('?', 2)[0]);
            }
            path = path.Replace('/', '\\');
            bool fullyQualified = Path.IsPathFullyQualified(path);
            if (string.IsNullOrWhiteSpace(path) || IsDevicePath(path) ||
                (Path.IsPathRooted(path) && !fullyQualified) || (!fullyQualified && path.Contains(':')))
                return false;
            localPath = path;
            return true;
        }
        catch (Exception error) when (error is UriFormatException or ArgumentException or
            NotSupportedException or PathTooLongException)
        {
            return false;
        }
    }

    private static bool IsDevicePath(string path)
    {
        var normalized = path.Replace('/', '\\');
        return normalized.StartsWith("\\\\?\\", StringComparison.Ordinal) ||
            normalized.StartsWith("\\\\.\\", StringComparison.Ordinal) ||
            normalized.StartsWith("\\??\\", StringComparison.Ordinal) ||
            normalized.StartsWith("\\\\??\\", StringComparison.Ordinal);
    }
}
