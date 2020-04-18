// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock

namespace Notepads.Controls
{
    using System;

    /// <summary>
    /// Arguments for the OnMarkdownRendered event which indicates when the markdown has been
    /// rendered.
    /// </summary>
    public class MarkdownRenderedEventArgs : EventArgs
    {
        internal MarkdownRenderedEventArgs(Exception ex)
        {
            Exception = ex;
        }

        /// <summary>
        /// Gets the exception if there was one. If the exception is null there was no error.
        /// </summary>
        public Exception Exception { get; }
    }
}