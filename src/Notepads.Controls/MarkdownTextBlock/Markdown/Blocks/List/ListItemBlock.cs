// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Blocks/List

namespace Notepads.Controls.Markdown
{
    using System.Collections.Generic;

    /// <summary>
    /// This specifies the Content of the List element.
    /// </summary>
    public class ListItemBlock
    {
        /// <summary>
        /// Gets or sets the contents of the list item.
        /// </summary>
        public IList<MarkdownBlock> Blocks { get; set; }

        internal ListItemBlock()
        {
        }
    }
}