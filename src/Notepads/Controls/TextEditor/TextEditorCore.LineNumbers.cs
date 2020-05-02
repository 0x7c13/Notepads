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
    using Notepads.Extensions;
    using Notepads.Utilities;

    public partial class TextEditorCore
    {
        private readonly Dictionary<int, TextBlock> _renderedLineNumberBlocks = new Dictionary<int, TextBlock>();
        private readonly Thickness _lineNumberPadding = new Thickness(6, 2, 4, 2);
        private readonly Dictionary<string, double> _lineNumberTextWidthCache = new Dictionary<string, double>();

        private void OnContentScrollViewerSizeChanged(object sender, SizeChangedEventArgs e)
        {
            RenderLineNumbers();
        }

        private void OnLineNumberGridSizeChanged(object sender, SizeChangedEventArgs e)
        {
            ResetLineNumberCanvasClipping();
        }

        private void ResetLineNumberCanvasClipping()
        {
            if (!_loaded) return;

            RectangleGeometry rectangle = new RectangleGeometry
            {
                Rect = new Rect(
                    0,
                    Padding.Top,
                    _lineNumberGrid.ActualWidth,
                    Math.Clamp(_lineNumberGrid.ActualHeight - (Padding.Top + Padding.Bottom), .0f, Double.PositiveInfinity))
            };
            _lineNumberGrid.Clip = rectangle;
        }

        private void RenderLineNumbers()
        {
            if (!_loaded) return;

            var startRange = Document.GetRangeFromPoint(
                new Point(_contentScrollViewer.HorizontalOffset, _contentScrollViewer.VerticalOffset),
                PointOptions.ClientCoordinates);

            var endRange = Document.GetRangeFromPoint(
                new Point(_contentScrollViewer.HorizontalOffset + _contentScrollViewer.ViewportWidth,
                    _contentScrollViewer.VerticalOffset + _contentScrollViewer.ViewportHeight),
                PointOptions.ClientCoordinates);

            UpdateContentLinesCacheIfNeeded();

            Dictionary<int, Rect> lineRects = CalculateLineRects(_contentLinesCache, startRange, endRange);

            var minLineNumberTextWidth = CalculateMinimumWidth(FontFamily, 
                FontSize, (_contentLinesCache.Length - 1).ToString().Length);

            RenderLineNumbersInternal(lineRects, minLineNumberTextWidth);

            _lineNumberGrid.Width = _lineNumberPadding.Left + minLineNumberTextWidth + _lineNumberPadding.Right;
        }

        private double CalculateMinimumWidth(FontFamily fontFamily, double fontSize, int textLength)
        {
            var cacheKey = $"{fontFamily.Source}-{(int)fontSize}-{textLength}";

            if (_lineNumberTextWidthCache.ContainsKey(cacheKey))
            {
                return _lineNumberTextWidthCache[cacheKey];
            }

            double minWidth = 0;

            for (int i = 0; i < 10; i++)
            {
                var str = new string((char)('0' + i), textLength);
                var width = FontUtility.GetTextSize(fontFamily, fontSize, str).Width;
                if (width > minWidth)
                {
                    minWidth = width;
                }
            }

            _lineNumberTextWidthCache[cacheKey] = minWidth;
            return minWidth;
        }

        private Dictionary<int, Rect> CalculateLineRects(string[] lines, ITextRange startRange, ITextRange endRange)
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

        private void RenderLineNumbersInternal(Dictionary<int, Rect> lineRects, double minLineNumberTextWidth)
        {
            foreach (var lineRect in lineRects)
            {
                var margin = new Thickness(_lineNumberPadding.Left,
                    lineRect.Value.Top + _lineNumberPadding.Top + Padding.Top,
                    _lineNumberPadding.Right,
                    _lineNumberPadding.Bottom);
                var height = (1.35 * FontSize) + Padding.Top + _lineNumberPadding.Top;

                var foreground = (ActualTheme == ElementTheme.Dark)
                    ? new SolidColorBrush("#99EEEEEE".ToColor())
                    : new SolidColorBrush("#99000000".ToColor());

                // Reposition already rendered line number blocks
                if (_renderedLineNumberBlocks.ContainsKey(lineRect.Key))
                {
                    _renderedLineNumberBlocks[lineRect.Key].Margin = margin;
                    _renderedLineNumberBlocks[lineRect.Key].Height = height;
                    _renderedLineNumberBlocks[lineRect.Key].Width = minLineNumberTextWidth;
                    _renderedLineNumberBlocks[lineRect.Key].Visibility = Visibility.Visible;
                    _renderedLineNumberBlocks[lineRect.Key].Foreground = foreground;
                }
                else // Render new line number block
                {
                    var lineNumberBlock = new TextBlock()
                    {
                        Text = lineRect.Key.ToString(),
                        Height = height,
                        Width = minLineNumberTextWidth,
                        Margin = margin,
                        TextAlignment = TextAlignment.Right,
                        HorizontalAlignment = HorizontalAlignment.Right,
                        VerticalAlignment = VerticalAlignment.Bottom,
                        HorizontalTextAlignment = TextAlignment.Right,
                        Foreground = foreground
                    };

                    _lineNumberCanvas.Children.Add(lineNumberBlock);
                    _renderedLineNumberBlocks.Add(lineRect.Key, lineNumberBlock);
                }
            }

            // Only show line number blocks within range (current ScrollViewer's viewport)
            // Hide all others to avoid rendering collision from happening
            foreach (var numberBlock in
                _renderedLineNumberBlocks.Where(x => !lineRects.ContainsKey(x.Key)))
            {
                numberBlock.Value.Visibility = Visibility.Collapsed;
            }
        }
    }
}