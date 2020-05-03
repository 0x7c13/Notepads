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
        private readonly Dictionary<string, double> _miniRequisiteIntegerTextRenderingWidthCache = new Dictionary<string, double>();

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

            var document = GetContentLinesCache();

            Dictionary<int, Rect> lineRects = CalculateLineRects(document, startRange, endRange);

            var minLineNumberTextRenderingWidth = CalculateMinimumRequisiteIntegerTextRenderingWidth(FontFamily, 
                FontSize, (document.Length - 1).ToString().Length);

            RenderLineNumbersInternal(lineRects, minLineNumberTextRenderingWidth);

            _lineNumberGrid.Width = _lineNumberPadding.Left + minLineNumberTextRenderingWidth + _lineNumberPadding.Right;
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

        private void RenderLineNumbersInternal(Dictionary<int, Rect> lineRects, double minLineNumberTextRenderingWidth)
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
                    _renderedLineNumberBlocks[lineRect.Key].Width = minLineNumberTextRenderingWidth;
                    _renderedLineNumberBlocks[lineRect.Key].Visibility = Visibility.Visible;
                    _renderedLineNumberBlocks[lineRect.Key].Foreground = foreground;
                }
                else // Render new line number block
                {
                    var lineNumberBlock = new TextBlock()
                    {
                        Text = lineRect.Key.ToString(),
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