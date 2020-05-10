// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    using System;
    using System.Collections.Generic;

    /// <summary>
    /// Represents an embedded image.
    /// </summary>
    public class ImageInline : MarkdownInline, IInlineLeaf
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="ImageInline"/> class.
        /// </summary>
        public ImageInline()
            : base(MarkdownInlineType.Image)
        {
        }

        /// <summary>
        /// Gets or sets the image URL.
        /// </summary>
        public string Url { get; set; }

        /// <summary>
        /// Gets or sets the image Render URL.
        /// </summary>
        public string RenderUrl { get; set; }

        /// <summary>
        /// Gets or sets a text to display on hover.
        /// </summary>
        public string Tooltip { get; set; }

        /// <inheritdoc/>
        public string Text { get; set; } = string.Empty;

        /// <summary>
        /// Gets or sets the ID of a reference, if this is a reference-style link.
        /// </summary>
        public string ReferenceId { get; set; }

        /// <summary>
        /// Gets image width
        /// If value is greater than 0, ImageStretch is set to UniformToFill
        /// If both ImageWidth and ImageHeight are greater than 0, ImageStretch is set to Fill
        /// </summary>
        public int ImageWidth { get; internal set; }

        /// <summary>
        /// Gets image height
        /// If value is greater than 0, ImageStretch is set to UniformToFill
        /// If both ImageWidth and ImageHeight are greater than 0, ImageStretch is set to Fill
        /// </summary>
        public int ImageHeight { get; internal set; }

        internal static void AddTripChars(List<InlineTripCharHelper> tripCharHelpers)
        {
            tripCharHelpers.Add(new InlineTripCharHelper() { FirstChar = '!', Method = InlineParseMethod.Image });
        }

        /// <summary>
        /// Attempts to parse an image e.g. "![Toolkit logo](https://raw.githubusercontent.com/windows-toolkit/WindowsCommunityToolkit/master/Microsoft.Toolkit.Uwp.SampleApp/Assets/ToolkitLogo.png)".
        /// </summary>
        /// <param name="markdown"> The markdown text. </param>
        /// <param name="start"> The location to start parsing. </param>
        /// <param name="end"> The location to stop parsing. </param>
        /// <returns> A parsed markdown image, or <c>null</c> if this is not a markdown image. </returns>
        internal static InlineParseResult Parse(string markdown, int start, int end)
        {
            // Expect a '!' character.
            if (start >= end || markdown[start] != '!')
            {
                return null;
            }

            int pos = start + 1;

            // Then a '[' character
            if (pos >= end || markdown[pos] != '[')
            {
                return null;
            }

            pos++;

            // Find the ']' character
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

            // Extract the alt.
            string tooltip = markdown.Substring(start + 2, pos - (start + 2));

            // Expect the '(' character.
            pos++;

            string reference = string.Empty;
            string url = string.Empty;
            int imageWidth = 0;
            int imageHeight = 0;

            if (pos < end && markdown[pos] == '[')
            {
                int refstart = pos;

                // Find the reference ']' character
                while (pos < end)
                {
                    if (markdown[pos] == ']')
                    {
                        break;
                    }

                    pos++;
                }

                reference = markdown.Substring(refstart + 1, pos - refstart - 1);
            }
            else if (pos < end && markdown[pos] == '(')
            {
                while (pos < end && ParseHelpers.IsMarkdownWhiteSpace(markdown[pos]))
                {
                    pos++;
                }

                // Extract the URL.
                int urlStart = pos;
                while (pos < end && markdown[pos] != ')')
                {
                    pos++;
                }

                var imageDimensionsPos = markdown.IndexOf(" =", urlStart, pos - urlStart, StringComparison.Ordinal);

                url = imageDimensionsPos > 0
                    ? TextRunInline.ResolveEscapeSequences(markdown, urlStart + 1, imageDimensionsPos)
                    : TextRunInline.ResolveEscapeSequences(markdown, urlStart + 1, pos);

                if (imageDimensionsPos > 0)
                {
                    // trying to find 'x' which separates image width and height
                    var dimensionsSepatorPos = markdown.IndexOf("x", imageDimensionsPos + 2, pos - imageDimensionsPos - 1, StringComparison.Ordinal);

                    // didn't find separator, trying to parse value as imageWidth
                    if (dimensionsSepatorPos == -1)
                    {
                        var imageWidthStr = markdown.Substring(imageDimensionsPos + 2, pos - imageDimensionsPos - 2);

                        _ = int.TryParse(imageWidthStr, out imageWidth);
                    }
                    else
                    {
                        var dimensions = markdown.Substring(imageDimensionsPos + 2, pos - imageDimensionsPos - 2).Split('x');

                        // got width and height
                        if (dimensions.Length == 2)
                        {
                            _ = int.TryParse(dimensions[0], out imageWidth);
                            _ = int.TryParse(dimensions[1], out imageHeight);
                        }
                    }
                }
            }

            if (pos == end)
            {
                return null;
            }

            // We found something!
            var result = new ImageInline
            {
                Tooltip = tooltip,
                RenderUrl = url,
                ReferenceId = reference,
                Url = url,
                Text = markdown.Substring(start, pos + 1 - start),
                ImageWidth = imageWidth,
                ImageHeight = imageHeight
            };
            return new InlineParseResult(result, start, pos + 1);
        }

        /// <summary>
        /// If this is a reference-style link, attempts to converts it to a regular link.
        /// </summary>
        /// <param name="document"> The document containing the list of references. </param>
        internal void ResolveReference(MarkdownDocument document)
        {
            if (document == null)
            {
                throw new ArgumentNullException(nameof(document));
            }

            if (ReferenceId == null)
            {
                return;
            }

            // Look up the reference ID.
            var reference = document.LookUpReference(ReferenceId);
            if (reference == null)
            {
                return;
            }

            // The reference was found. Check the URL is valid.
            if (!Common.IsUrlValid(reference.Url))
            {
                return;
            }

            // Everything is cool when you're part of a team.
            RenderUrl = reference.Url;
            ReferenceId = null;
        }

        /// <summary>
        /// Converts the object into it's textual representation.
        /// </summary>
        /// <returns> The textual representation of this object. </returns>
        public override string ToString()
        {
            if (ImageWidth > 0 && ImageHeight > 0)
            {
                return $"![{Tooltip}]: {Url} (Width: {ImageWidth}, Height: {ImageHeight})";
            }

            if (ImageWidth > 0)
            {
                return $"![{Tooltip}]: {Url} (Width: {ImageWidth})";
            }

            if (ImageHeight > 0)
            {
                return $"![{Tooltip}]: {Url} (Height: {ImageHeight})";
            }

            return $"![{Tooltip}]: {Url}";
        }
    }
}