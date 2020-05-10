// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock/Render

namespace Notepads.Controls.Markdown
{
    using Windows.UI.Xaml.Documents;

    /// <summary>
    /// The Context of the Current Document Rendering.
    /// </summary>
    public class InlineRenderContext : RenderContext
    {
        internal InlineRenderContext(InlineCollection inlineCollection, IRenderContext context)
        {
            InlineCollection = inlineCollection;
            TrimLeadingWhitespace = context.TrimLeadingWhitespace;
            Parent = context.Parent;

            if (context is RenderContext localcontext)
            {
                Foreground = localcontext.Foreground;
                OverrideForeground = localcontext.OverrideForeground;
            }

            if (context is InlineRenderContext inlinecontext)
            {
                WithinBold = inlinecontext.WithinBold;
                WithinItalics = inlinecontext.WithinItalics;
                WithinHyperlink = inlinecontext.WithinHyperlink;
            }
        }

        /// <summary>
        /// Gets or sets a value indicating whether the Current Element is being rendered inside an Italics Run.
        /// </summary>
        public bool WithinItalics { get; set; }

        /// <summary>
        /// Gets or sets a value indicating whether the Current Element is being rendered inside a Bold Run.
        /// </summary>
        public bool WithinBold { get; set; }

        /// <summary>
        /// Gets or sets a value indicating whether the Current Element is being rendered inside a Link.
        /// </summary>
        public bool WithinHyperlink { get; set; }

        /// <summary>
        /// Gets or sets the list to add to.
        /// </summary>
        public InlineCollection InlineCollection { get; set; }
    }
}