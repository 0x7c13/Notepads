
namespace Notepads.Extensions
{
    using Microsoft.Toolkit.Uwp.UI.Controls;
    using Notepads.Controls.TextEditor;
    using System;
    using System.IO;
    using System.Net;
    using System.Threading.Tasks;
    using Windows.Storage.Streams;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Media.Imaging;

    public class Downloader : IDisposable
    {
        public async Task<MemoryStream> GetDataFeed(string feedUrl)
        {
            var request = (HttpWebRequest)WebRequest.Create(feedUrl);
            request.Method = "GET";

            var response = (HttpWebResponse)await request.GetResponseAsync();

            MemoryStream ms = new MemoryStream();
            var stream = response.GetResponseStream();
            stream?.CopyTo(ms);
            return ms;
        }

        public void Dispose() { }
    }

    public sealed partial class MarkdownExtensionView : IContentPreviewExtension
    {
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
            MarkdownTextBlock.ImageResolving += MarkdownTextBlock_ImageResolving;
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
            catch (Exception)
            {
                e.Handled = false;
            }

            deferral.Complete();
        }

        private async Task<ImageSource> GetImageAsync(string url)
        {
            var imageUri = new Uri(url);
            using (var d = new Downloader())
            {
                var feed = await d.GetDataFeed(url);
                feed.Seek(0, SeekOrigin.Begin);

                using (InMemoryRandomAccessStream ms = new InMemoryRandomAccessStream())
                {
                    using (DataWriter writer = new DataWriter(ms.GetOutputStreamAt(0)))
                    {
                        writer.WriteBytes((byte[])feed.ToArray());
                        writer.StoreAsync().GetResults();
                    }

                    if (Path.GetExtension(imageUri.AbsolutePath)?.ToLowerInvariant() == ".svg")
                    {
                        var image = new SvgImageSource();
                        await image.SetSourceAsync(ms);
                        return image;
                    }
                    else
                    {
                        var image = new BitmapImage();
                        await image.SetSourceAsync(ms);
                        return image;
                    }
                }
            }
        }

        public void Bind(TextEditorCore editor)
        {
            if (_editorCore != null)
            {
                _editorCore.TextChanged -= OnTextChanged;
                _editorCore.TextWrappingChanged -= OnTextWrappingChanged;
            }
            _editorCore = editor;
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
            catch (Exception)
            {
                // Ignore
            }
        }
    }
}
