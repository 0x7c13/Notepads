// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Enums

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// The alignment of content in a table column.
    /// </summary>
    public enum ColumnAlignment
    {
        /// <summary>
        /// The alignment was not specified.
        /// </summary>
        Unspecified,

        /// <summary>
        /// Content should be left aligned.
        /// </summary>
        Left,

        /// <summary>
        /// Content should be right aligned.
        /// </summary>
        Right,

        /// <summary>
        /// Content should be centered.
        /// </summary>
        Center,
    }
}