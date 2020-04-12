// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock/Render

namespace Notepads.Controls.Markdown
{
    using System;

    /// <summary>
    /// An Exception that occurs when the Render Context is Incorrect.
    /// </summary>
    public class RenderContextIncorrectException : Exception
    {
        internal RenderContextIncorrectException()
            : base("Markdown Render Context missing or incorrect.")
        {
        }
    }
}