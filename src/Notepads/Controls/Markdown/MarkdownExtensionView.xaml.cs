﻿// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.Markdown
{
    using Controls;
    using Extensions;
    using Microsoft.Toolkit.Uwp.UI;
    using Services;
    using System;
    using System.IO;
    using System.Threading.Tasks;
    using TextEditor;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    public sealed partial class MarkdownExtensionView : UserControl, IContentPreviewExtension
    {
        private readonly ImageCache _imageCache = new ImageCache();

        private bool _isExtensionEnabled;

        public bool IsExtensionEnabled
        {
            get => _isExtensionEnabled;
            set
            {
                _isExtensionEnabled = value;
                if (_isExtensionEnabled)
                {
                    UpdateFontSize();
                    UpdateTextWrapping();
                    UpdateText();
                }
            }
        }

        private TextEditorCore _editorCore;

        public MarkdownExtensionView()
        {
            InitializeComponent();

            MarkdownTextBlock.LinkClicked += MarkdownTextBlock_OnLinkClicked;
            MarkdownTextBlock.ImageClicked += MarkdownTextBlock_OnLinkClicked;
            MarkdownTextBlock.ImageResolving += MarkdownTextBlock_ImageResolving;

            ThemeSettingsService.OnThemeChanged += OnThemeChanged;
        }

        private async void OnThemeChanged(object sender, ElementTheme theme)
        {
            await Dispatcher.CallOnUIThreadAsync(() =>
            {
                MarkdownTextBlock.RequestedTheme = theme;
            });
        }

        public void Dispose()
        {
            IsExtensionEnabled = false;

            MarkdownTextBlock.LinkClicked -= MarkdownTextBlock_OnLinkClicked;
            MarkdownTextBlock.ImageClicked -= MarkdownTextBlock_OnLinkClicked;
            MarkdownTextBlock.ImageResolving -= MarkdownTextBlock_ImageResolving;
            MarkdownTextBlock.Text = string.Empty;

            _editorCore.TextChanged -= OnTextChanged;
            _editorCore.TextWrappingChanged -= OnTextWrappingChanged;
            _editorCore.FontSizeChanged -= OnFontSizeChanged;

            ThemeSettingsService.OnThemeChanged -= OnThemeChanged;

            Task.Run(async () => { await _imageCache.ClearAsync(); });
        }

        private async void MarkdownTextBlock_ImageResolving(object sender, ImageResolvingEventArgs e)
        {
            var deferral = e.GetDeferral();

            try
            {
                var imageUri = new Uri(e.Url);
                if (Path.GetExtension(imageUri.AbsolutePath)?.ToLowerInvariant() == ".svg")
                {
                    // SvgImageSource is not working properly when width and height are not set in uri
                    // I am disabling Svg parsing here.
                    // e.Image = await GetImageAsync(e.Url);
                    e.Handled = true;
                }
                else
                {
                    e.Image = await GetImageAsync(e.Url);
                    e.Handled = true;
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(MarkdownExtensionView)}] Failed to resolve Markdown image [{e.Url}]: {ex.Message}");
                e.Handled = false;
            }

            deferral.Complete();
        }

        private async Task<ImageSource> GetImageAsync(string url)
        {
            var imageUri = new Uri(url);

            return await _imageCache.GetFromCacheAsync(imageUri);

            //var feed = await Downloader.GetDataFeed(url);
            //feed.Seek(0, SeekOrigin.Begin);

            //using (InMemoryRandomAccessStream ms = new InMemoryRandomAccessStream())
            //{
            //    using (DataWriter writer = new DataWriter(ms.GetOutputStreamAt(0)))
            //    {
            //        writer.WriteBytes(feed.ToArray());
            //        writer.StoreAsync().GetResults();
            //    }

            //    if (Path.GetExtension(imageUri.AbsolutePath)?.ToLowerInvariant() == ".svg")
            //    {
            //        var image = new SvgImageSource();
            //        await image.SetSourceAsync(ms);
            //        return image;
            //    }
            //    else
            //    {
            //        var image = new BitmapImage();
            //        await image.SetSourceAsync(ms);
            //        return image;
            //    }
            //}
        }

        public void Bind(TextEditorCore editorCore)
        {
            if (_editorCore != null)
            {
                _editorCore.TextChanged -= OnTextChanged;
                _editorCore.TextWrappingChanged -= OnTextWrappingChanged;
                _editorCore.FontSizeChanged -= OnFontSizeChanged;
            }

            _editorCore = editorCore;
            _editorCore.TextChanged += OnTextChanged;
            _editorCore.TextWrappingChanged += OnTextWrappingChanged;
            _editorCore.FontSizeChanged += OnFontSizeChanged;
        }

        private void OnTextChanged(object sender, RoutedEventArgs e)
        {
            if (IsExtensionEnabled)
            {
                UpdateText();
            }
        }

        private void OnTextWrappingChanged(object sender, TextWrapping textWrapping)
        {
            if (IsExtensionEnabled)
            {
                UpdateTextWrapping();
            }
        }

        private void OnFontSizeChanged(object sender, double fontSize)
        {
            if (IsExtensionEnabled)
            {
                UpdateFontSize();
            }
        }

        private void UpdateFontSize()
        {
            if (_editorCore != null)
            {
                MarkdownTextBlock.FontSize = _editorCore.FontSize;
                MarkdownTextBlock.Header1FontSize = MarkdownTextBlock.FontSize + 5;
                MarkdownTextBlock.Header2FontSize = MarkdownTextBlock.FontSize + 5;
                MarkdownTextBlock.Header3FontSize = MarkdownTextBlock.FontSize + 2;
                MarkdownTextBlock.Header4FontSize = MarkdownTextBlock.FontSize + 2;
                MarkdownTextBlock.Header5FontSize = MarkdownTextBlock.FontSize + 1;
                MarkdownTextBlock.Header6FontSize = MarkdownTextBlock.FontSize + 1;
            }
        }

        private void UpdateText()
        {
            if (_editorCore != null)
            {
                MarkdownTextBlock.ImageMaxWidth = ActualWidth;
                MarkdownTextBlock.Text = _editorCore.GetText();
            }
        }

        private void UpdateTextWrapping()
        {
            if (_editorCore != null)
            {
                MarkdownTextBlock.TextWrapping = _editorCore.TextWrapping;
                MarkdownScrollViewer.HorizontalScrollBarVisibility = MarkdownTextBlock.TextWrapping == TextWrapping.Wrap ? ScrollBarVisibility.Disabled : ScrollBarVisibility.Visible;
            }
        }

        private async void MarkdownTextBlock_OnLinkClicked(object sender, LinkClickedEventArgs e)
        {
            if (string.IsNullOrEmpty(e.Link))
            {
                return;
            }

            try
            {
                var uri = new Uri(e.Link);
                await Windows.System.Launcher.LaunchUriAsync(uri);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(MarkdownExtensionView)}] Failed to open Markdown Link: {ex.Message}");
            }
        }
    }
}
