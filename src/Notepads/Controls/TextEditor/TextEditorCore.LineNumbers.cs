namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.Foundation;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Notepads.Utilities;
    using Microsoft.Toolkit.Uwp.Helpers;

    public partial class TextEditorCore
    {
        public bool _displayLineNumbers;

        public bool DisplayLineNumbers
        {
            get => _displayLineNumbers;
            set
            {
                if (_displayLineNumbers != value)
                {
                    _displayLineNumbers = value;

                    if (value)
                    {
                        ShowLineNumbers();
                    }
                    else
                    {
                        HideLineNumbers();
                    }
                }
            }
        }

        private readonly Dictionary<int, TextBlock> _renderedLineNumberBlocks = new Dictionary<int, TextBlock>();
        private readonly Dictionary<string, double> _miniRequisiteIntegerTextRenderingWidthCache = new Dictionary<string, double>();
        private readonly SolidColorBrush _lineNumberDarkModeForegroundBrush = new SolidColorBrush("#99EEEEEE".ToColor());
        private readonly SolidColorBrush _lineNumberLightModeForegroundBrush = new SolidColorBrush("#99000000".ToColor());

        public void ShowLineNumbers()
        {
            if (!_loaded) return;

            ResetLineNumberCanvasClipping();
            UpdateLineNumbersRendering();

            // Call UpdateLineHighlighterAndIndicator to adjust it's state
            UpdateLineHighlighterAndIndicator();
        }

        public void HideLineNumbers()
        {
            if (!_loaded) return;

            _lineNumberCanvas?.Children.Clear();
            _renderedLineNumberBlocks.Clear();
            _miniRequisiteIntegerTextRenderingWidthCache.Clear();

            _lineNumberGrid.BorderThickness = new Thickness(0, 0, 0, 0);
            _lineNumberGrid.Margin = new Thickness(0, 0, 0, 0);
            _lineNumberGrid.Width = .0f;

            // Call UpdateLineHighlighterAndIndicator to adjust it's state
            // Since when line highlighter is disabled, we still show the line indicator when line numbers are showing
            UpdateLineHighlighterAndIndicator();
        }

        private void OnLineNumberGridSizeChanged(object sender, SizeChangedEventArgs e)
        {
            ResetLineNumberCanvasClipping();
        }

        private void ResetLineNumberCanvasClipping()
        {
            if (!_loaded || !DisplayLineNumbers) return;

            _lineNumberGrid.Margin = new Thickness(0, 0, -1 * Padding.Left + 1, 0);
            _lineNumberGrid.Clip = new RectangleGeometry
            {
                Rect = new Rect(
                    0,
                    Padding.Top,
                    _lineNumberGrid.ActualWidth,
                    Math.Clamp(_lineNumberGrid.ActualHeight - (Padding.Top + Padding.Bottom), .0f, Double.PositiveInfinity))
            };
        }

        private void UpdateLineNumbersRendering()
        {
            if (!_loaded || !DisplayLineNumbers) return;

            var startRange = Document.GetRangeFromPoint(
                new Point(_contentScrollViewer.HorizontalOffset, _contentScrollViewer.VerticalOffset),
                PointOptions.ClientCoordinates);

            var endRange = Document.GetRangeFromPoint(
                new Point(_contentScrollViewer.HorizontalOffset + _contentScrollViewer.ViewportWidth,
                    _contentScrollViewer.VerticalOffset + _contentScrollViewer.ViewportHeight),
                PointOptions.ClientCoordinates);

            var document = GetDocumentLinesCache();

            Dictionary<int, Rect> lineNumberTextRenderingPositions = CalculateLineNumberTextRenderingPositions(document, startRange, endRange);

            var minLineNumberTextRenderingWidth = CalculateMinimumRequisiteIntegerTextRenderingWidth(FontFamily,
                FontSize, (document.Length - 1).ToString().Length);

            RenderLineNumbersInternal(lineNumberTextRenderingPositions, minLineNumberTextRenderingWidth);
        }

        /// <summary>
        /// Get minimum rendering width needed for displaying number text with certain length.
        /// Take length of 3 as example, it is going to iterate thru all possible combinations like:
        /// 111, 222, 333, 444 ... 999 to get minimum rendering length needed to display all of them (the largest width is the min here).
        /// For mono font text, the width is always the same for same length but for non-mono font text, it depends.
        /// Thus we need to calculate here to determine width needed for rendering integer number only text.
        /// </summary>
        /// <param name="fontFamily"></param>
        /// <param name="fontSize"></param>
        /// <param name="numberTextLength"></param>
        /// <returns></returns>
        private double CalculateMinimumRequisiteIntegerTextRenderingWidth(FontFamily fontFamily, double fontSize, int numberTextLength)
        {
            var cacheKey = $"{fontFamily.Source}-{(int)fontSize}-{numberTextLength}";

            if (_miniRequisiteIntegerTextRenderingWidthCache.ContainsKey(cacheKey))
            {
                return _miniRequisiteIntegerTextRenderingWidthCache[cacheKey];
            }

            double minRequisiteWidth = 0;

            for (int i = 0; i < 10; i++)
            {
                var str = new string((char)('0' + i), numberTextLength);
                var width = FontUtility.GetTextSize(fontFamily, fontSize, str).Width;
                if (width > minRequisiteWidth)
                {
                    minRequisiteWidth = width;
                }
            }

            _miniRequisiteIntegerTextRenderingWidthCache[cacheKey] = minRequisiteWidth;
            return minRequisiteWidth;
        }

        private Dictionary<int, Rect> CalculateLineNumberTextRenderingPositions(string[] lines, ITextRange startRange, ITextRange endRange)
        {
            var offset = 0;
            var lineRects = new Dictionary<int, Rect>(); // 1 - based

            for (int i = 0; i < lines.Length - 1; i++)
            {
                var line = lines[i];

                // Use "offset + line.Length + 1" instead of just "offset" here is to capture the line right above the viewport
                if (offset + line.Length + 1 >= startRange.StartPosition && offset <= endRange.EndPosition)
                {
                    Document.GetRange(offset, offset + line.Length)
                        .GetRect(PointOptions.ClientCoordinates, out var rect, out _);

                    lineRects[i + 1] = rect;
                }
                else if (offset > endRange.EndPosition)
                {
                    break;
                }

                offset += line.Length + 1; // 1 for line ending: '\r'
            }

            return lineRects;
        }

        private void RenderLineNumbersInternal(Dictionary<int, Rect> lineNumberTextRenderingPositions, double minLineNumberTextRenderingWidth)
        {
            var padding = FontSize / 2;
            var lineNumberPadding = new Thickness(padding, 2, padding + 2, 2 );

            foreach (var lineNumber in lineNumberTextRenderingPositions)
            {
                var margin = new Thickness(lineNumberPadding.Left,
                    lineNumber.Value.Top + lineNumberPadding.Top + Padding.Top,
                    lineNumberPadding.Right,
                    lineNumberPadding.Bottom);
                var height = GetSingleLineHeight() + Padding.Top + lineNumberPadding.Top;

                _lineNumberGrid.BorderThickness = new Thickness(0, 0, 0.08 * GetSingleLineHeight(), 0);

                var foreground = (ActualTheme == ElementTheme.Dark)
                    ? _lineNumberDarkModeForegroundBrush
                    : _lineNumberLightModeForegroundBrush;

                // Reposition already rendered line number blocks
                if (_renderedLineNumberBlocks.ContainsKey(lineNumber.Key))
                {
                    _renderedLineNumberBlocks[lineNumber.Key].Margin = margin;
                    _renderedLineNumberBlocks[lineNumber.Key].Height = height;
                    _renderedLineNumberBlocks[lineNumber.Key].Width = minLineNumberTextRenderingWidth;
                    _renderedLineNumberBlocks[lineNumber.Key].Visibility = Visibility.Visible;
                    _renderedLineNumberBlocks[lineNumber.Key].Foreground = foreground;
                }
                else // Render new line number block
                {
                    var lineNumberBlock = new TextBlock()
                    {
                        Text = lineNumber.Key.ToString(),
                        Height = height,
                        Width = minLineNumberTextRenderingWidth,
                        Margin = margin,
                        TextAlignment = TextAlignment.Right,
                        HorizontalAlignment = HorizontalAlignment.Right,
                        VerticalAlignment = VerticalAlignment.Bottom,
                        HorizontalTextAlignment = TextAlignment.Right,
                        Foreground = foreground
                    };

                    _lineNumberCanvas.Children.Add(lineNumberBlock);
                    _renderedLineNumberBlocks.Add(lineNumber.Key, lineNumberBlock);
                }
            }

            // Only show line number blocks within range (current ScrollViewer's viewport)
            // Hide all others to avoid rendering collision from happening
            foreach (var numberBlock in
                _renderedLineNumberBlocks.Where(x => !lineNumberTextRenderingPositions.ContainsKey(x.Key)))
            {
                numberBlock.Value.Visibility = Visibility.Collapsed;
            }

            _lineNumberGrid.Width = lineNumberPadding.Left + minLineNumberTextRenderingWidth + lineNumberPadding.Right;
        }
    }
}