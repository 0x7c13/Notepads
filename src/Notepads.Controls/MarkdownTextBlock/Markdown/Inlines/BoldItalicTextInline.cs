// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    using System.Collections.Generic;

    /// <summary>
    /// Represents a span containing bold italic text.
    /// </summary>
    internal class BoldItalicTextInline : MarkdownInline, IInlineContainer
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="BoldItalicTextInline"/> class.
        /// </summary>
        public BoldItalicTextInline()
            : base(MarkdownInlineType.Bold)
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
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '*', Method = InlineParseMethod.BoldItalic });
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '_', Method = InlineParseMethod.BoldItalic });
        }

        /// <summary>
        /// Attempts to parse a bold text span.
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location to start parsing. </param>
        /// <param name="maxEnd"> The location to stop parsing. </param>
        /// <returns> A parsed bold text span, or <c>null</c> if this is not a bold text span. </returns>
        internal static InlineParseResult Parse(string markdown, int start, int maxEnd)
        {
            if (start >= maxEnd - 1)
            {
                return null;
            }

            if (markdown == null || markdown.Length < 6 || start + 6 >= maxEnd)
            {
                return null;
            }

            // Check the start sequence.
            string startSequence = markdown.Substring(start, 3);
            if (startSequence != "***" && startSequence != "___")
            {
                return null;
            }

            // Find the end of the span.  The end sequence (either '***' or '___') must be the same
            // as the start sequence.
            var innerStart = start + 3;
            int innerEnd = Common.IndexOf(markdown, startSequence, innerStart, maxEnd);
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
            var bold = new BoldTextInline
            {
                Inlines = new List<MarkdownInline>
                {
                    new ItalicTextInline
                    {
                        Inlines = Common.ParseInlineChildren(markdown, innerStart, innerEnd)
                    }
                }
            };
            return new InlineParseResult(bold, start, innerEnd + 3);
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

            return "***" + string.Join(string.Empty, Inlines) + "***";
        }
    }
}
