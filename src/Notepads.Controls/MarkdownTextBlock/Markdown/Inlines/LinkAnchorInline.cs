// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    using System.Collections.Generic;
    using System.Xml.Linq;

    /// <summary>
    /// Represents a span that contains a reference for links to point to.
    /// </summary>
    public class LinkAnchorInline : MarkdownInline
    {
        internal LinkAnchorInline()
            : base(MarkdownInlineType.LinkReference)
        {
        }

        /// <summary>
        /// Gets or sets the Name of this Link Reference.
        /// </summary>
        public string Link { get; set; }

        /// <summary>
        /// Gets or sets the raw Link Reference.
        /// </summary>
        public string Raw { get; set; }

        /// <summary>
        /// Returns the chars that if found means we might have a match.
        /// </summary>
        internal static void AddTripChars(List<InlineTripCharHelper> tripCharHelpers)
        {
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '<', Method = InlineParseMethod.LinkReference });
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

            // Check the start sequence.
            string startSequence = markdown.Substring(start, 2);
            if (startSequence != "<a")
            {
                return null;
            }

            // Find the end of the span.  The end sequence ('-->')
            var innerStart = start + 2;
            int innerEnd = Common.IndexOf(markdown, "</a>", innerStart, maxEnd);
            int trueEnd = innerEnd + 4;
            if (innerEnd == -1)
            {
                innerEnd = Common.IndexOf(markdown, "/>", innerStart, maxEnd);
                trueEnd = innerEnd + 2;
                if (innerEnd == -1)
                {
                    return null;
                }
            }

            // This link Reference wasn't closed properly if the next link starts before a close.
            var nextLink = Common.IndexOf(markdown, "<a", innerStart, maxEnd);
            if (nextLink > -1 && nextLink < innerEnd)
            {
                return null;
            }

            var length = trueEnd - start;
            var contents = markdown.Substring(start, length);

            string link = null;

            try
            {
                var xml = XElement.Parse(contents);
                var attr = xml.Attribute("name");
                if (attr != null)
                {
                    link = attr.Value;
                }
            }
            catch
            {
                // Attempting to fetch link failed, ignore and continue.
            }

            // Remove whitespace if it exists.
            if (trueEnd + 1 <= maxEnd && markdown[trueEnd] == ' ')
            {
                trueEnd += 1;
            }

            // We found something!
            var result = new LinkAnchorInline
            {
                Raw = contents,
                Link = link
            };
            return new InlineParseResult(result, start, trueEnd);
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            return Raw;
        }
    }
}