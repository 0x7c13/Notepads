// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.IO;
using System.Linq;
using System.Text.RegularExpressions;

namespace Notepads.Features.Documents.IO;

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

public static class FileNameRules
{
    // https://stackoverflow.com/questions/62771/how-do-i-check-if-a-given-string-is-a-legal-valid-file-name-under-windows
    private static readonly Regex ValidWindowsFileNames = new(@"^(?!(?:PRN|AUX|CLOCK\$|NUL|CON|COM\d|LPT\d)(?:\..+)?$)[^\x00-\x1F\xA5\\?*:\"";|\/<>]+(?<![\s.])$", RegexOptions.IgnoreCase);

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

}
