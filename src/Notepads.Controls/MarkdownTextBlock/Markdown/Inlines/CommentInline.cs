// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    using System;
    using System.Collections.Generic;

    /// <summary>
    /// Represents a span that contains comment.
    /// </summary>
    internal class CommentInline : MarkdownInline, IInlineLeaf
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="CommentInline"/> class.
        /// </summary>
        public CommentInline()
            : base(MarkdownInlineType.Comment)
        {
        }

        /// <summary>
        /// Gets or sets the Content of the Comment.
        /// </summary>
        public string Text { get; set; }

        /// <summary>
        /// Returns the chars that if found means we might have a match.
        /// </summary>
        internal static void AddTripChars(List<InlineTripCharHelper> tripCharHelpers)
        {
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '<', Method = InlineParseMethod.Comment });
        }

        /// <summary>
        /// Attempts to parse a comment span.
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

            string startSequence = markdown.Substring(start);
            if (!startSequence.StartsWith("<!--", StringComparison.Ordinal))
            {
                return null;
            }

            // Find the end of the span.  The end sequence ('-->')
            var innerStart = start + 4;
            int innerEnd = Common.IndexOf(markdown, "-->", innerStart, maxEnd);
            if (innerEnd == -1)
            {
                return null;
            }

            var length = innerEnd - innerStart;
            var contents = markdown.Substring(innerStart, length);

            var result = new CommentInline
            {
                Text = contents
            };

            return new InlineParseResult(result, start, innerEnd + 3);
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            return "<!--" + Text + "-->";
        }
    }
}