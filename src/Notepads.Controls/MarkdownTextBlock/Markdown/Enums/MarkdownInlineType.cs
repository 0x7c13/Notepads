// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Enums

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// Determines the type of Inline the Inline Element is.
    /// </summary>
    public enum MarkdownInlineType
    {
        /// <summary>
        /// A comment
        /// </summary>
        Comment,

        /// <summary>
        /// A text run
        /// </summary>
        TextRun,

        /// <summary>
        /// A bold run
        /// </summary>
        Bold,

        /// <summary>
        /// An italic run
        /// </summary>
        Italic,

        /// <summary>
        /// A link in markdown syntax
        /// </summary>
        MarkdownLink,

        /// <summary>
        /// A raw hyper link
        /// </summary>
        RawHyperlink,

        /// <summary>
        /// A raw subreddit link
        /// </summary>
        RawSubreddit,

        /// <summary>
        /// A strike through run
        /// </summary>
        Strikethrough,

        /// <summary>
        /// A superscript run
        /// </summary>
        Superscript,

        /// <summary>
        /// A subscript run
        /// </summary>
        Subscript,

        /// <summary>
        /// A code run
        /// </summary>
        Code,

        /// <summary>
        /// An image
        /// </summary>
        Image,

        /// <summary>
        /// Emoji
        /// </summary>
        Emoji,

        /// <summary>
        /// Link Reference
        /// </summary>
        LinkReference
    }
}