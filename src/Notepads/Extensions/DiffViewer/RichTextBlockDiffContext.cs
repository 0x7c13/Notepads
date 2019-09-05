namespace Notepads.Extensions.DiffViewer
{
    using System.Collections.Generic;
    using Windows.UI;
    using Windows.UI.Xaml.Documents;

    public class RichTextBlockDiffContext
    {
        private bool _hasPendingHightlighter;
        private int _lastStart;
        private int _lastEnd;
        private Color _lastHightlightColor;

        public RichTextBlockDiffContext()
        {
            Blocks = new List<Block>();
            TextHighlighters = new List<TextHighlighter>();
        }

        public IList<Block> Blocks { get; set; }

        private IList<TextHighlighter> TextHighlighters { get; set; }

        public void QueuePendingHightlighter(TextRange textRange, Color backgroundColor)
        {
            _lastStart = textRange.StartIndex;
            _lastEnd = _lastStart + textRange.Length;
            _lastHightlightColor = backgroundColor;
            _hasPendingHightlighter = true;
        }

        public void AddTextHighlighter(TextRange textRange, Color backgroundColor)
        {
            if (!_hasPendingHightlighter)
            {
                QueuePendingHightlighter(textRange, backgroundColor);
            }
            else
            {
                if (_lastEnd == textRange.StartIndex && _lastHightlightColor == backgroundColor)
                {
                    _lastEnd += textRange.Length;
                }
                else
                {
                    TextRange range = new TextRange() { StartIndex = _lastStart, Length = _lastEnd - _lastStart };
                    TextHighlighters.Add(new TextHighlighter()
                    {
                        Background = BrushFactory.GetSolidColorBrush(_lastHightlightColor),
                        Ranges = { range }
                    });
                    QueuePendingHightlighter(textRange, backgroundColor);
                }
            }
        }

        public IList<TextHighlighter> GetTextHighlighters()
        {
            if (_hasPendingHightlighter)
            {
                TextRange range = new TextRange() { StartIndex = _lastStart, Length = _lastEnd - _lastStart };
                TextHighlighters.Add(new TextHighlighter()
                {
                    Background = BrushFactory.GetSolidColorBrush(_lastHightlightColor),
                    Ranges = { range }
                });
                _hasPendingHightlighter = false;
            }
            return TextHighlighters;
        }
    }
}