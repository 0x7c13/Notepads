namespace Notepads.Controls.TextEditor
{
    using Windows.UI.Xaml;

    public partial class TextEditorCore
    {
        public bool _displayLineHighlighter;

        public bool DisplayLineHighlighter
        {
            get => _displayLineHighlighter;
            set
            {
                if (_displayLineHighlighter != value)
                {
                    _displayLineHighlighter = value;
                    UpdateLineHighlighterAndIndicator();
                }
            }
        }

        public void UpdateLineHighlighterAndIndicator()
        {
            if (!_loaded) return;

            if (!DisplayLineHighlighter && !DisplayLineNumbers)
            {
                _lineHighlighter.Visibility = Visibility.Collapsed;
                _lineIndicator.Visibility = Visibility.Collapsed;
                return;
            }

            Document.Selection.GetRect(Windows.UI.Text.PointOptions.ClientCoordinates,
                out Windows.Foundation.Rect selectionRect, out var _);

            var singleLineHeight = GetSingleLineHeight();
            var thickness = new Thickness(0.08 * singleLineHeight);

            // Show line highlighter rect when it is enabled when selection is single line only
            if (DisplayLineHighlighter && selectionRect.Height < singleLineHeight * 1.5f)
            {
                _lineHighlighter.Height = selectionRect.Height;
                _lineHighlighter.Margin = new Thickness(0, selectionRect.Y + Padding.Top, 0, 0);
                _lineHighlighter.BorderThickness = thickness;
                _lineHighlighter.Width = _rootGrid.ActualWidth;

                _lineHighlighter.Visibility = Visibility.Visible;
                _lineIndicator.Visibility = Visibility.Collapsed;
            }
            else if (DisplayLineNumbers) // Show line indicator when line highlighter is disabled but line numbers are enabled
            {
                _lineIndicator.Height = selectionRect.Height;
                _lineIndicator.Margin = new Thickness(0, selectionRect.Y + Padding.Top, 0, 0);
                _lineIndicator.BorderThickness = thickness;
                _lineIndicator.Width = 0.1 * singleLineHeight;

                _lineIndicator.Visibility = Visibility.Visible;
                _lineHighlighter.Visibility = Visibility.Collapsed;
            }
            else
            {
                _lineIndicator.Visibility = Visibility.Collapsed;
                _lineHighlighter.Visibility = Visibility.Collapsed;
            }
        }
    }
}