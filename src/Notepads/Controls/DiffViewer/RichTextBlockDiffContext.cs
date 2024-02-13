// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.DiffViewer
{
    using System.Collections.Generic;
    using System.Linq;
    using Windows.UI;
    using Windows.UI.Xaml.Documents;

    public class RichTextBlockDiffContext
    {
        private bool _hasPendingHighlighter;
        private int _lastStart;
        private int _lastEnd;
        private Color _lastHighlightColor;

        private readonly BrushFactory _brushFactory;
        private readonly Dictionary<Color, TextHighlighter> _textHighlighters = new Dictionary<Color, TextHighlighter>();

        public RichTextBlockDiffContext(BrushFactory brushFactory)
        {
            Blocks = new List<Block>();
            _brushFactory = brushFactory;
        }

        public IList<Block> Blocks { get; set; }

        public void QueuePendingHighlighter(TextRange textRange, Color backgroundColor)
        {
            _lastStart = textRange.StartIndex;
            _lastEnd = _lastStart + textRange.Length;
            _lastHighlightColor = backgroundColor;
            _hasPendingHighlighter = true;
        }

        public void AddTextHighlighter(TextRange textRange, Color backgroundColor)
        {
            if (!_hasPendingHighlighter)
            {
                QueuePendingHighlighter(textRange, backgroundColor);
            }
            else
            {
                if (_lastEnd == textRange.StartIndex && _lastHighlightColor == backgroundColor)
                {
                    _lastEnd += textRange.Length;
                }
                else
                {
                    TextRange range = new TextRange() { StartIndex = _lastStart, Length = _lastEnd - _lastStart };
                    AddOrUpdateTextHighlighterInternal(_lastHighlightColor, range);
                    QueuePendingHighlighter(textRange, backgroundColor);
                }
            }
        }

        public IList<TextHighlighter> GetTextHighlighters()
        {
            if (_hasPendingHighlighter)
            {
                TextRange range = new TextRange() { StartIndex = _lastStart, Length = _lastEnd - _lastStart };
                AddOrUpdateTextHighlighterInternal(_lastHighlightColor, range);
                _hasPendingHighlighter = false;
            }
            return _textHighlighters.Values.ToList();
        }

        private void AddOrUpdateTextHighlighterInternal(Color backgroundColor, TextRange range)
        {
            if (_textHighlighters.ContainsKey(backgroundColor))
            {
                _textHighlighters[backgroundColor].Ranges.Add(range);
            }
            else
            {
                _textHighlighters[backgroundColor] = new TextHighlighter()
                {
                    Background = _brushFactory.GetOrCreateSolidColorBrush(backgroundColor),
                    Ranges = { range }
                };
            }
        }
    }
}