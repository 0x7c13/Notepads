// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Blocks

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// Represents the target of a reference ([ref][]).
    /// </summary>
    public class LinkReferenceBlock : MarkdownBlock
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="LinkReferenceBlock"/> class.
        /// </summary>
        public LinkReferenceBlock()
            : base(MarkdownBlockType.LinkReference)
        {
        }

        /// <summary>
        /// Gets or sets the reference ID.
        /// </summary>
        public string Id { get; set; }

        /// <summary>
        /// Gets or sets the link URL.
        /// </summary>
        public string Url { get; set; }

        /// <summary>
        /// Gets or sets a tooltip to display on hover.
        /// </summary>
        public string Tooltip { get; set; }

        /// <summary>
        /// Attempts to parse a reference e.g. "[example]: http://www.reddit.com 'title'".
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location to start parsing. </param>
        /// <param name="end"> The location to stop parsing. </param>
        /// <returns> A parsed markdown link, or <c>null</c> if this is not a markdown link. </returns>
        internal static LinkReferenceBlock Parse(string markdown, int start, int end)
        {
            // Expect a '[' character.
            if (start >= end || markdown[start] != '[')
            {
                return null;
            }

            // Find the ']' character
            int pos = start + 1;
            while (pos < end)
            {
                if (markdown[pos] == ']')
                {
                    break;
                }

                pos++;
            }

            if (pos == end)
            {
                return null;
            }

            // Extract the ID.
            string id = markdown.Substring(start + 1, pos - (start + 1));

            // Expect the ':' character.
            pos++;
            if (pos == end || markdown[pos] != ':')
            {
                return null;
            }

            // Skip whitespace
            pos++;
            while (pos < end && ParseHelpers.IsMarkdownWhiteSpace(markdown[pos]))
            {
                pos++;
            }

            if (pos == end)
            {
                return null;
            }

            // Extract the URL.
            int urlStart = pos;
            while (pos < end && !ParseHelpers.IsMarkdownWhiteSpace(markdown[pos]))
            {
                pos++;
            }

            string url = TextRunInline.ResolveEscapeSequences(markdown, urlStart, pos);

            // Ignore leading '<' and trailing '>'.
            url = url.TrimStart('<').TrimEnd('>');

            // Skip whitespace.
            pos++;
            while (pos < end && ParseHelpers.IsMarkdownWhiteSpace(markdown[pos]))
            {
                pos++;
            }

            string tooltip = null;
            if (pos < end)
            {
                // Extract the tooltip.
                char tooltipEndChar;
                switch (markdown[pos])
                {
                    case '(':
                        tooltipEndChar = ')';
                        break;

                    case '"':
                    case '\'':
                        tooltipEndChar = markdown[pos];
                        break;

                    default:
                        return null;
                }

                pos++;
                int tooltipStart = pos;
                while (pos < end && markdown[pos] != tooltipEndChar)
                {
                    pos++;
                }

                if (pos == end)
                {
                    return null;    // No end character.
                }

                tooltip = markdown.Substring(tooltipStart, pos - tooltipStart);

                // Check there isn't any trailing text.
                pos++;
                while (pos < end && ParseHelpers.IsMarkdownWhiteSpace(markdown[pos]))
                {
                    pos++;
                }

                if (pos < end)
                {
                    return null;
                }
            }

            // We found something!
            var result = new LinkReferenceBlock {Id = id, Url = url, Tooltip = tooltip};
            return result;
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            return $"[{Id}]: {Url} {Tooltip}";
        }
    }
}