// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Parsers/Markdown/Render

namespace Notepads.Controls.Markdown
{
    /// <summary>
    /// Helper for holding persistent state of Renderer.
    /// </summary>
    public interface IRenderContext
    {
        /// <summary>
        /// Gets or sets a value indicating whether to trim whitespace.
        /// </summary>
        bool TrimLeadingWhitespace { get; set; }

        /// <summary>
        /// Gets or sets the parent Element for this Context.
        /// </summary>
        object Parent { get; set; }

        /// <summary>
        /// Clones the Context.
        /// </summary>
        /// <returns>Clone</returns>
        IRenderContext Clone();
    }
}