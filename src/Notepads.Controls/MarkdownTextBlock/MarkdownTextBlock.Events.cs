// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/MarkdownTextBlock

namespace Notepads.Controls
{
    using System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Documents;

    /// <summary>
    /// An efficient and extensible control that can parse and render markdown.
    /// </summary>
    public partial class MarkdownTextBlock
    {
        /// <summary>
        /// Calls OnPropertyChanged.
        /// </summary>
        private static void OnPropertyChangedStatic(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            var instance = d as MarkdownTextBlock;

            // Defer to the instance method.
            instance?.OnPropertyChanged(d, e.Property);
        }

        /// <summary>
        /// Fired when the value of a DependencyProperty is changed.
        /// </summary>
        private void OnPropertyChanged(DependencyObject d, DependencyProperty prop)
        {
            RenderMarkdown();
        }

        /// <summary>
        /// Fired when a user taps one of the link elements
        /// </summary>
        private void Hyperlink_Click(Hyperlink sender, HyperlinkClickEventArgs args)
        {
            LinkHandled((string)sender.GetValue(HyperlinkUrlProperty), true);
        }

        /// <summary>
        /// Fired when a user taps one of the image elements
        /// </summary>
        private void NewImagelink_Tapped(object sender, Windows.UI.Xaml.Input.TappedRoutedEventArgs e)
        {
            string hyperLink = (string)(sender as Image).GetValue(HyperlinkUrlProperty);
            bool isHyperLink = (bool)(sender as Image).GetValue(IsHyperlinkProperty);
            LinkHandled(hyperLink, isHyperLink);
        }

        /// <summary>
        /// Fired when the text is done parsing and formatting. Fires each time the markdown is rendered.
        /// </summary>
        public event EventHandler<MarkdownRenderedEventArgs> MarkdownRendered;

        /// <summary>
        /// Fired when a link element in the markdown was tapped.
        /// </summary>
        public event EventHandler<LinkClickedEventArgs> LinkClicked;

        /// <summary>
        /// Fired when an image element in the markdown was tapped.
        /// </summary>
        public event EventHandler<LinkClickedEventArgs> ImageClicked;

        /// <summary>
        /// Fired when an image from the markdown document needs to be resolved.
        /// The default implementation is basically <code>new BitmapImage(new Uri(e.Url));</code>.
        /// <para/>You must set <see cref="ImageResolvingEventArgs.Handled"/> to true in order to process your changes.
        /// </summary>
        public event EventHandler<ImageResolvingEventArgs> ImageResolving;

        /// <summary>
        /// Fired when a Code Block is being Rendered.
        /// The default implementation is to output the CodeBlock as Plain Text.
        /// <para/>You must set <see cref="CodeBlockResolvingEventArgs.Handled"/> to true in order to process your changes.
        /// </summary>
        public event EventHandler<CodeBlockResolvingEventArgs> CodeBlockResolving;
    }
}