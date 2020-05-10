// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    using System.Collections.Generic;

    /// <summary>
    /// Represents a span containing italic text.
    /// </summary>
    public class ItalicTextInline : MarkdownInline, IInlineContainer
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="ItalicTextInline"/> class.
        /// </summary>
        public ItalicTextInline()
            : base(MarkdownInlineType.Italic)
        {
        }

        /// <summary>
        /// Gets or sets the contents of the inline.
        /// </summary>
        public IList<MarkdownInline> Inlines { get; set; }

        /// <summary>
        /// Returns the chars that if found means we might have a match.
        /// </summary>
        internal static void AddTripChars(List<InlineTripCharHelper> tripCharHelpers)
        {
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '*', Method = InlineParseMethod.Italic });
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '_', Method = InlineParseMethod.Italic });
        }

        /// <summary>
        /// Attempts to parse a italic text span.
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location to start parsing. </param>
        /// <param name="maxEnd"> The location to stop parsing. </param>
        /// <returns> A parsed italic text span, or <c>null</c> if this is not a italic text span. </returns>
        internal static InlineParseResult Parse(string markdown, int start, int maxEnd)
        {
            // Check the first char.
            char startChar = markdown[start];
            if (start == maxEnd || (startChar != '*' && startChar != '_'))
            {
                return null;
            }

            // Find the end of the span.  The end character (either '*' or '_') must be the same as
            // the start character.
            var innerStart = start + 1;
            int innerEnd = Common.IndexOf(markdown, startChar, start + 1, maxEnd);
            if (innerEnd == -1)
            {
                return null;
            }

            // The span must contain at least one character.
            if (innerStart == innerEnd)
            {
                return null;
            }

            // The first character inside the span must NOT be a space.
            if (ParseHelpers.IsMarkdownWhiteSpace(markdown[innerStart]))
            {
                return null;
            }

            // The last character inside the span must NOT be a space.
            if (ParseHelpers.IsMarkdownWhiteSpace(markdown[innerEnd - 1]))
            {
                return null;
            }

            // We found something!
            var result = new ItalicTextInline
            {
                Inlines = Common.ParseInlineChildren(markdown, innerStart, innerEnd)
            };
            return new InlineParseResult(result, start, innerEnd + 1);
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            if (Inlines == null)
            {
                return base.ToString();
            }

            return "*" + string.Join(string.Empty, Inlines) + "*";
        }
    }
}