
namespace Notepads.Extensions
{
    using Microsoft.Toolkit.Uwp.UI.Controls;
    using Notepads.Controls.TextEditor;
    using System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

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

            var uri = new Uri(e.Link);
            await Windows.System.Launcher.LaunchUriAsync(uri);
        }
    }
}
