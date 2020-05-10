// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Blocks/List

namespace Notepads.Controls.Markdown
{
    internal class NestedListInfo
    {
        public ListBlock List { get; set; }

        // The number of spaces at the start of the line the list first appeared.
        public int SpaceCount { get; set; }
    }
}