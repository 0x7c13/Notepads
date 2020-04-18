// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock

namespace Notepads.Controls
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Threading.Tasks;
    using Windows.Foundation;
    using Windows.UI.Xaml.Media;

    /// <summary>
    /// Arguments for the <see cref="MarkdownTextBlock.ImageResolving"/> event which is called when a url needs to be resolved to a <see cref="ImageSource"/>.
    /// </summary>
    public class ImageResolvingEventArgs : EventArgs
    {
        private readonly IList<TaskCompletionSource<object>> _deferrals;

        internal ImageResolvingEventArgs(string url, string tooltip)
        {
            _deferrals = new List<TaskCompletionSource<object>>();
            Url = url;
            Tooltip = tooltip;
        }

        /// <summary>
        /// Gets the url of the image in the markdown document.
        /// </summary>
        public string Url { get; }

        /// <summary>
        /// Gets the tooltip of the image in the markdown document.
        /// </summary>
        public string Tooltip { get; }

        /// <summary>
        /// Gets or sets a value indicating whether this event was handled successfully.
        /// </summary>
        public bool Handled { get; set; }

        /// <summary>
        /// Gets or sets the image to display in the <see cref="MarkdownTextBlock"/>.
        /// </summary>
        public ImageSource Image { get; set; }

        /// <summary>
        /// Informs the <see cref="MarkdownTextBlock"/> that the event handler might run asynchronously.
        /// </summary>
        /// <returns>Deferral</returns>
        public Deferral GetDeferral()
        {
            var task = new TaskCompletionSource<object>();
            _deferrals.Add(task);

            return new Deferral(() =>
            {
                task.SetResult(null);
            });
        }

        /// <summary>
        /// Returns a <see cref="Task"/> that completes when all <see cref="Deferral"/>s have completed.
        /// </summary>
        /// <returns>A <see cref="Task"/> representing the asynchronous operation.</returns>
        internal Task WaitForDeferrals()
        {
            return Task.WhenAll(_deferrals.Select(f => f.Task));
        }
    }
}