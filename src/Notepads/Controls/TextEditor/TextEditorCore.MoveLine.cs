namespace Notepads.Controls.TextEditor
{
    using Windows.UI.Text;

    public partial class TextEditorCore
    {
        private void MoveLinesUp()
        {
            GetTextSelectionPosition(out var start, out var end);
            GetLineColumnSelection(out var startLine,
                out var endLine,
                out var startColumn,
                out var endColumn,
                out _,
                out _);

            if (startLine == 1) return;

            var document = GetText();
            var lines = GetDocumentLinesCache();

            var startLineInitialIndex = start - startColumn;
            var endLineFinalIndex = end - endColumn + lines[endLine - 1].Length +
                (end > start && (end > document.Length || document[end - 1].Equals(RichEditBoxDefaultLineEnding)) ? 0 : 1);

            var selectionMoveAmount = lines[startLine - 2].Length + 1;
            var insertIndex = startLineInitialIndex - selectionMoveAmount;
            var movingLines = document.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);
            var remainingContent = document.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);

            if (insertIndex <= 0)
            {
                insertIndex = 0;
                remainingContent = RichEditBoxDefaultLineEnding + remainingContent;
                movingLines = movingLines.TrimStart(RichEditBoxDefaultLineEnding);
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = remainingContent.Insert(insertIndex, movingLines);
            start -= selectionMoveAmount;
            end -= selectionMoveAmount;
            if (start < 0) start = 0;

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original scroll position after changing the lines' position in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    horizontalOffset,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }

        private void MoveLinesDown()
        {
            GetTextSelectionPosition(out var start, out var end);
            GetLineColumnSelection(out _,
                out var endLine,
                out var startColumn,
                out var endColumn,
                out _,
                out var lineCount);

            if (endLine == lineCount) return;

            var document = GetText();
            var lines = GetDocumentLinesCache();

            var startLineInitialIndex = start - startColumn + 1;
            var endLineFinalIndex = end - endColumn + lines[endLine - 1].Length +
                (end > start && document[end - 1].Equals(RichEditBoxDefaultLineEnding) ? 1 : 2);

            var selectionMoveAmount = lines[endLine].Length + 1;
            var insertIndex = startLineInitialIndex + selectionMoveAmount;
            var movingLines = document.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);
            var remainingContent = document.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);

            if (insertIndex >= remainingContent.Length)
            {
                remainingContent += RichEditBoxDefaultLineEnding;
                movingLines = movingLines.TrimEnd(RichEditBoxDefaultLineEnding);
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect,
               out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = remainingContent.Insert(insertIndex, movingLines);
            start += selectionMoveAmount;
            end += selectionMoveAmount;
            if (end > document.Length) end = document.Length;

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original scroll position after changing the lines' position in this case
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