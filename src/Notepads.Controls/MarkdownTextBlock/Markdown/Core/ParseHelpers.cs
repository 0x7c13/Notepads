// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/blob/master/Microsoft.Toolkit.Parsers/Core

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// This class offers helpers for Parsing.
    /// </summary>
    public static class ParseHelpers
    {
        /// <summary>
        /// Determines if a Markdown string is blank or comprised entirely of whitespace characters.
        /// </summary>
        /// <returns>true if blank or white space</returns>
        public static bool IsMarkdownBlankOrWhiteSpace(string str)
        {
            for (int i = 0; i < str.Length; i++)
            {
                if (!IsMarkdownWhiteSpace(str[i]))
                {
                    return false;
                }
            }

            return true;
        }

        /// <summary>
        /// Determines if a character is a Markdown whitespace character.
        /// </summary>
        /// <returns>true if is white space</returns>
        public static bool IsMarkdownWhiteSpace(char c)
        {
            return c == ' ' || c == '\t' || c == '\r' || c == '\n';
        }
    }
}