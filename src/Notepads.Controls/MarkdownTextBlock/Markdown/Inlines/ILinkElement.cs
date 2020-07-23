// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Inlines

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// Implemented by all inline link elements.
    /// </summary>
    internal interface ILinkElement
    {
        /// <summary>
        /// Gets the link URL.  This can be a relative URL, but note that subreddit links will always
        /// have the leading slash (i.e. the Url will be "/r/baconit" even if the text is
        /// "r/baconit").
        /// </summary>
        string Url { get; }

        /// <summary>
        /// Gets a tooltip to display on hover.  Can be <c>null</c>.
        /// </summary>
        string Tooltip { get; }
    }
}