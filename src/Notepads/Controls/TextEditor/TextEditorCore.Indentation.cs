namespace Notepads.Controls.TextEditor
{
    using System.Text;
    using Windows.UI.Text;

    public partial class TextEditorCore
    {
        /// <summary>
        /// Add indentation based on current selection
        /// </summary>
        /// <param name="indent">
        /// indent == -1 meaning '/t' should be used, otherwise it equals to number of spaces
        /// </param>
        private void AddIndentation(int indent)
        {
            GetTextSelectionPosition(out var start, out var end);
            GetLineColumnSelection(out var startLine, out var endLine, out var startColumn, out var endColumn, out _,
                out _);

            var tabStr = indent == -1 ? "\t" : new string(' ', indent);

            // Handle single line selection scenario where part of the line is selected
            if (startLine == endLine)
            {
                Document.Selection.TypeText(tabStr);
                Document.Selection.StartPosition = Document.Selection.EndPosition;
                return;
            }

            var startLineInitialIndex = start - startColumn + 1;
            var endLineFinalIndex = end - endColumn + _contentLinesCache[endLine - 1].Length + 1;
            if (endLineFinalIndex > _content.Length) endLineFinalIndex = _content.Length;

            var indentAmount = indent == -1 ? 1 : indent;
            start += indentAmount;

            var indentedStringBuilder = new StringBuilder();
            for (var i = startLine - 1; i < endLine; i++)
            {
                indentedStringBuilder.Append(string.Concat(tabStr, _contentLinesCache[i],
                    i < endLine - 1 ? RichEditBoxDefaultLineEnding.ToString() : string.Empty));
                end += indentAmount;
            }

            if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                indentedStringBuilder.ToString())) return;

            if (Document.Selection.Text.EndsWith(RichEditBoxDefaultLineEnding) && endLineFinalIndex < _content.Length)
            {
                indentedStringBuilder.Append(RichEditBoxDefaultLineEnding);
                if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                    indentedStringBuilder.ToString())) return;
            }

            Document.Selection.GetRect(Windows.UI.Text.PointOptions.Transform, out Windows.Foundation.Rect rect,
                out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = _content.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex)
                .Insert(startLineInitialIndex, indentedStringBuilder.ToString());

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original scroll position after changing the indent in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    horizontalOffset,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }

        /// <summary>
        /// Remove indentation based on current selection
        /// </summary>
        /// <param name="indent">
        /// indent == -1 meaning '/t' should be used, otherwise it equals to number of spaces
        /// </param>
        private void RemoveIndentation(int indent)
        {
            GetTextSelectionPosition(out var start, out var end);
            GetLineColumnSelection(out var startLine, out var endLine, out var startColumn, out var endColumn, out _,
                out _);

            var startLineInitialIndex = start - startColumn + 1;
            var endLineFinalIndex = end - endColumn + _contentLinesCache[endLine - 1].Length + 1;
            if (endLineFinalIndex > _content.Length) endLineFinalIndex = _content.Length;

            if (startLineInitialIndex == endLineFinalIndex) return;

            var indentedStringBuilder = new StringBuilder();
            for (var i = startLine - 1; i < endLine; i++)
            {
                var lineTrailingString = i < endLine - 1 ? RichEditBoxDefaultLineEnding.ToString() : string.Empty;
                if (_contentLinesCache[i].StartsWith('\t'))
                {
                    indentedStringBuilder.Append(_contentLinesCache[i].Remove(0, 1) + lineTrailingString);
                    end--;
                }
                else
                {
                    var spaceCount = 0;
                    var indentAmount = indent == -1 ? 4 : indent;

                    for (var charIndex = 0;
                        charIndex < _contentLinesCache[i].Length && _contentLinesCache[i][charIndex] == ' ';
                        charIndex++)
                    {
                        spaceCount++;
                    }

                    if (spaceCount == 0)
                    {
                        indentedStringBuilder.Append(_contentLinesCache[i] + lineTrailingString);
                        continue;
                    }

                    var insufficientSpace = spaceCount % indentAmount;

                    if (insufficientSpace > 0)
                    {
                        indentedStringBuilder.Append(_contentLinesCache[i].Remove(0, insufficientSpace) +
                                                     lineTrailingString);
                        end -= insufficientSpace;
                    }
                    else
                    {
                        indentedStringBuilder.Append(_contentLinesCache[i].Remove(0, indentAmount) +
                                                     lineTrailingString);
                        end -= indentAmount;
                    }
                }

                if (i == startLine - 1)
                {
                    if (startLine == endLine)
                        start -= _contentLinesCache[i].Length - indentedStringBuilder.Length;
                    else
                        start -= _contentLinesCache[i].Length - indentedStringBuilder.Length + 1;

                    if (start < startLineInitialIndex)
                    {
                        if (end == start) end = startLineInitialIndex;
                        start = startLineInitialIndex;
                    }
                }
            }

            if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                indentedStringBuilder.ToString())) return;

            if (Document.Selection.Text.EndsWith(RichEditBoxDefaultLineEnding) && endLineFinalIndex < _content.Length)
            {
                indentedStringBuilder.Append(RichEditBoxDefaultLineEnding);
                if (string.Equals(_content.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex),
                    indentedStringBuilder.ToString())) return;
            }

            Document.Selection.GetRect(Windows.UI.Text.PointOptions.Transform, out Windows.Foundation.Rect rect,
                out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = _content.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex)
                .Insert(startLineInitialIndex, indentedStringBuilder.ToString());

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original scroll position after changing the indent in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    horizontalOffset,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }
    }
}