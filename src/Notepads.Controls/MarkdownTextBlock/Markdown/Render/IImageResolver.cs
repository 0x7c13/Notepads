// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock/Render

namespace Notepads.Controls.Markdown
{
    using System.Threading.Tasks;
    using Windows.UI.Xaml.Media;

    /// <summary>
    /// An interface used to resolve images in the markdown.
    /// </summary>
    public interface IImageResolver
    {
        /// <summary>
        /// Resolves an Image from a Url.
        /// </summary>
        /// <param name="url">Url to Resolve.</param>
        /// <param name="tooltip">Tooltip for Image.</param>
        /// <returns>Image</returns>
        Task<ImageSource> ResolveImageAsync(string url, string tooltip);
    }
}