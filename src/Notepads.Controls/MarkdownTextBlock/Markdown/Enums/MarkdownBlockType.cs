// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Enums

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// Determines the type of Block the Block element is.
    /// </summary>
    public enum MarkdownBlockType
    {
        /// <summary>
        /// The root element
        /// </summary>
        Root,

        /// <summary>
        /// A paragraph element.
        /// </summary>
        Paragraph,

        /// <summary>
        /// A quote block
        /// </summary>
        Quote,

        /// <summary>
        /// A code block
        /// </summary>
        Code,

        /// <summary>
        /// A header block
        /// </summary>
        Header,

        /// <summary>
        /// A list block
        /// </summary>
        List,

        /// <summary>
        /// A list item block
        /// </summary>
        ListItemBuilder,

        /// <summary>
        /// a horizontal rule block
        /// </summary>
        HorizontalRule,

        /// <summary>
        /// A table block
        /// </summary>
        Table,

        /// <summary>
        /// A link block
        /// </summary>
        LinkReference,

        /// <summary>
        /// A Yaml header block
        /// </summary>
        YamlHeader
    }
}