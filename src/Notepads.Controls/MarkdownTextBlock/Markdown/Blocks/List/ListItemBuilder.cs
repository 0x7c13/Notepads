// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Blocks/List

namespace Notepads.Controls.Markdown
{
    using System.Text;

    internal class ListItemBuilder : MarkdownBlock
    {
        public StringBuilder Builder { get; } = new StringBuilder();

        public ListItemBuilder()
            : base(MarkdownBlockType.ListItemBuilder)
        {
        }
    }
}