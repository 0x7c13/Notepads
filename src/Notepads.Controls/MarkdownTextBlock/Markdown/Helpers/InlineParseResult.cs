// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Helpers

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// Represents the result of parsing an inline element.
    /// </summary>
    internal class InlineParseResult
    {
        public InlineParseResult(MarkdownInline parsedElement, int start, int end)
        {
            ParsedElement = parsedElement;
            Start = start;
            End = end;
        }

        /// <summary>
        /// Gets the element that was parsed (can be <c>null</c>).
        /// </summary>
        public MarkdownInline ParsedElement { get; }

        /// <summary>
        /// Gets the position of the first character in the parsed element.
        /// </summary>
        public int Start { get; }

        /// <summary>
        /// Gets the position of the character after the last character in the parsed element.
        /// </summary>
        public int End { get; }
    }
}