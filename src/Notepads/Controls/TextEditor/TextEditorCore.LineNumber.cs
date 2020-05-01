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
            RectangleGeometry rectangle = new RectangleGeometry
            {
                Rect = new Rect(
                    0,
                    Padding.Top,
                    _lineNumberGrid.ActualWidth,
                    Math.Clamp(_lineNumberGrid.ActualHeight - (Padding.Top * 2), .0f, Double.PositiveInfinity))
            };
            _lineNumberGrid.Clip = rectangle;
        }

        private void RenderLineNumbers()
        {
            if (_contentScrollViewer == null) return;

            var startRange = Document.GetRangeFromPoint(
                new Point(_contentScrollViewer.HorizontalOffset, _contentScrollViewer.VerticalOffset),
                PointOptions.ClientCoordinates);

            var endRange = Document.GetRangeFromPoint(
                new Point(_contentScrollViewer.HorizontalOffset + _contentScrollViewer.ViewportWidth,
                    _contentScrollViewer.VerticalOffset + _contentScrollViewer.ViewportHeight),
                PointOptions.ClientCoordinates);

            var lines = _content.Split(RichEditBoxDefaultLineEnding);

            Dictionary<int, Rect> lineRects = CalculateLineRects(lines, startRange, endRange);

            RenderLineNumbersInternal(lineRects);

            _lineNumberGrid.Width = FontUtility.GetTextSize(FontFamily, FontSize,
                                        (lines.Length).ToString()).Width + Padding.Right;
        }

        private Dictionary<int, Rect> CalculateLineRects(string[] lines, ITextRange startRange, ITextRange endRange)
        {
            var offset = 0;
            var lineIndex = 1;
            var lineRects = new Dictionary<int, Rect>();

            foreach (var line in lines)
            {
                if (offset >= startRange.StartPosition && offset <= endRange.EndPosition)
                {
                    Document.GetRange(offset, offset + line.Length)
                        .GetRect(PointOptions.ClientCoordinates, out var rect, out _);

                    lineRects[lineIndex] = rect;
                }
                else if (offset > endRange.EndPosition)
                {
                    break;
                }

                offset += line.Length + 1;
                lineIndex++;
            }

            return lineRects;
        }

        private void RenderLineNumbersInternal(Dictionary<int, Rect> lineRects)
        {
            // Render diff
            foreach (var lineRect in lineRects)
            {
                var margin = new Thickness(Padding.Left, lineRect.Value.Top + Padding.Top + 2, Padding.Right, 0);
                var height = (1.35 * FontSize) + Padding.Top + 2;

                var foreground = (ActualTheme == ElementTheme.Dark)
                    ? new SolidColorBrush("#99EEEEEE".ToColor())
                    : new SolidColorBrush("#99000000".ToColor());

                if (_renderedLineNumberBlocks.ContainsKey(lineRect.Key))
                {
                    _renderedLineNumberBlocks[lineRect.Key].Margin = margin;
                    _renderedLineNumberBlocks[lineRect.Key].Height = height;
                    _renderedLineNumberBlocks[lineRect.Key].Visibility = Visibility.Visible;
                    _renderedLineNumberBlocks[lineRect.Key].Foreground = foreground;
                }
                else
                {
                    var lineNumberBlock = new TextBlock()
                    {
                        Text = lineRect.Key.ToString(),
                        Height = height,
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

            // Only show line number blocks within range
            // Hide all others to avoid rendering collision from happening
            foreach (var numberBlock in
                _renderedLineNumberBlocks.Where(x => !lineRects.ContainsKey(x.Key)))
            {
                numberBlock.Value.Visibility = Visibility.Collapsed;
            }
        }
    }
}