// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Enums

namespace Notepads.Controls.Markdown
{
    internal enum InlineParseMethod
    {
        /// <summary>
        /// A Comment text
        /// </summary>
        Comment,

        /// <summary>
        /// A Link Reference
        /// </summary>
        LinkReference,

        /// <summary>
        /// A bold element
        /// </summary>
        Bold,

        /// <summary>
        /// An bold and italic block
        /// </summary>
        BoldItalic,

        /// <summary>
        /// A code element
        /// </summary>
        Code,

        /// <summary>
        /// An italic block
        /// </summary>
        Italic,

        /// <summary>
        /// A link block
        /// </summary>
        MarkdownLink,

        /// <summary>
        /// An angle bracket link.
        /// </summary>
        AngleBracketLink,

        /// <summary>
        /// A url block
        /// </summary>
        Url,

        /// <summary>
        /// A reddit style link
        /// </summary>
        RedditLink,

        /// <summary>
        /// An in line text link
        /// </summary>
        PartialLink,

        /// <summary>
        /// An email element
        /// </summary>
        Email,

        /// <summary>
        /// strike through element
        /// </summary>
        Strikethrough,

        /// <summary>
        /// Super script element.
        /// </summary>
        Superscript,

        /// <summary>
        /// Sub script element.
        /// </summary>
        Subscript,

        /// <summary>
        /// Image element.
        /// </summary>
        Image,

        /// <summary>
        /// Emoji element.
        /// </summary>
        Emoji
    }
}