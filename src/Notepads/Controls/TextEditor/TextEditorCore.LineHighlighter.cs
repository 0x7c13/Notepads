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
                out Windows.Foundation.Rect lineRect, out var _);

            var lineHeight = GetSingleLineHeight();
            var thickness = new Thickness(0.08 * lineHeight);

            // Show line highlighter rect when it is enabled
            if (DisplayLineHighlighter)
            {
                _lineHighlighter.Height = lineHeight;
                _lineHighlighter.Margin = new Thickness(0, lineRect.Y + Padding.Top, 0, 0);
                _lineHighlighter.BorderThickness = thickness;
                _lineHighlighter.Width = _rootGrid.ActualWidth;

                _lineHighlighter.Visibility = Visibility.Visible;
                _lineIndicator.Visibility = Visibility.Collapsed;
            }
            else // Show line indicator when line highlighter is disabled
            {
                _lineIndicator.Height = lineHeight;
                _lineIndicator.Margin = new Thickness(0, lineRect.Y + Padding.Top, 0, 0);
                _lineIndicator.BorderThickness = thickness;
                _lineIndicator.Width = 0.1 * lineHeight;

                _lineIndicator.Visibility = Visibility.Visible;
                _lineHighlighter.Visibility = Visibility.Collapsed;
            }
        }
    }
}