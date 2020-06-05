namespace Notepads.Controls.TextEditor
{
    using Notepads.Services;
    using Windows.UI.Text;

    public partial class TextEditorCore
    {
        private void MoveTextUp()
        {
            GetLineColumnSelection(out var startLine,
                out var endLine,
                out var startColumn,
                out var endColumn,
                out _,
                out _);

            if (startLine == 1) return;

            GetTextSelectionPosition(out var start, out var end);

            var document = GetText();
            var lines = GetDocumentLinesCache();

            var startLineInitialIndex = start - startColumn;
            var endLineFinalIndex = end - endColumn + lines[endLine - 1].Length +
                (end > start && (end > document.Length || document[end - 1].Equals(RichEditBoxDefaultLineEnding)) ? 0 : 1);

            var selectionMoveAmount = lines[startLine - 2].Length + 1;
            var insertIndex = startLineInitialIndex - selectionMoveAmount;
            var movingText = document.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);
            var remainingContent = document.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);

            if (insertIndex < 0)
            {
                insertIndex = 0;
                remainingContent = RichEditBoxDefaultLineEnding + remainingContent;
                movingText = movingText.Remove(0, 1);
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = remainingContent.Insert(insertIndex, movingText);
            start -= selectionMoveAmount;
            end -= selectionMoveAmount;
            if (start < 0) start = 0;

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view
            // and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original scroll position after changing the texts' position in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    horizontalOffset,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }

        private void MoveTextDown()
        {
            GetLineColumnSelection(out _,
                out var endLine,
                out var startColumn,
                out var endColumn,
                out _,
                out var lineCount);

            if (endLine == lineCount) return;

            GetTextSelectionPosition(out var start, out var end);

            var document = GetText();
            var lines = GetDocumentLinesCache();

            var startLineInitialIndex = start - startColumn + 1;
            var endLineFinalIndex = end - endColumn + lines[endLine - 1].Length +
                (end > start && document[end - 1].Equals(RichEditBoxDefaultLineEnding) ? 1 : 2);

            var selectionMoveAmount = lines[endLine].Length + 1;
            var insertIndex = startLineInitialIndex + selectionMoveAmount;
            var movingText = document.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);
            var remainingContent = document.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);

            if (insertIndex >= remainingContent.Length)
            {
                remainingContent += RichEditBoxDefaultLineEnding;
                movingText = movingText.Remove(movingText.Length - 1);
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = remainingContent.Insert(insertIndex, movingText);
            start += selectionMoveAmount;
            end += selectionMoveAmount;

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view
            // and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original scroll position after changing the texts' position in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    horizontalOffset,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }

        private void MoveTextLeft()
        {
            GetTextSelectionPosition(out var start, out var end);

            var document = GetText();

            if (start == 0) return;

            var startIndex = start;
            if (end == start || char.IsLetterOrDigit(document[start]))
            {
                while (startIndex > 0)
                {
                    startIndex--;
                    if (!char.IsLetterOrDigit(document[startIndex]))
                    {
                        startIndex++;
                        break;
                    }
                }
            }
            if (startIndex <= 0) return;

            if (end > document.Length) end = document.Length;
            var endIndex = end;
            if (end == start || char.IsLetterOrDigit(document[end - 1]))
            {
                while (endIndex < document.Length)
                {
                    endIndex++;
                    if (!char.IsLetterOrDigit(document[endIndex - 1]))
                    {
                        endIndex--;
                        break;
                    }
                }
            }
            if (startIndex >= endIndex) return;

            var replacedWordEndIndex = startIndex;
            while (replacedWordEndIndex > 0)
            {
                replacedWordEndIndex--;
                if (char.IsLetterOrDigit(document[replacedWordEndIndex]))
                {
                    replacedWordEndIndex++;
                    break;
                }
            }

            var replacedWordStartIndex = replacedWordEndIndex;
            while (replacedWordStartIndex > 0)
            {
                replacedWordStartIndex--;
                if (!char.IsLetterOrDigit(document[replacedWordStartIndex]))
                {
                    replacedWordStartIndex++;
                    break;
                }
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);
            
            var movingText = document.Substring(startIndex, endIndex - startIndex);
            var replacedWord = document.Substring(replacedWordStartIndex, replacedWordEndIndex - replacedWordStartIndex);
            var selectionMoveAmount = startIndex - replacedWordStartIndex;
            document = document.Remove(startIndex, endIndex - startIndex).Insert(startIndex, replacedWord)
                .Remove(replacedWordStartIndex, replacedWordEndIndex - replacedWordStartIndex).Insert(replacedWordStartIndex, movingText);
            start -= selectionMoveAmount;
            end -= selectionMoveAmount;

            Document.SetText(TextSetOptions.None, document);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view
            // and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original vertical scroll position after changing the texts' position in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    null,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }

        private void MoveTextRight()
        {
            GetTextSelectionPosition(out var start, out var end);

            var document = GetText();

            if (end >= document.Length) return;

            var startIndex = start;
            if (end == start || char.IsLetterOrDigit(document[start]))
            {
                while (startIndex > 0)
                {
                    startIndex--;
                    if (!char.IsLetterOrDigit(document[startIndex]))
                    {
                        startIndex++;
                        break;
                    }
                }
            }

            var endIndex = end;
            if (end == start || char.IsLetterOrDigit(document[end - 1]))
            {
                while (endIndex < document.Length)
                {
                    endIndex++;
                    if (!char.IsLetterOrDigit(document[endIndex - 1]))
                    {
                        endIndex--;
                        break;
                    }
                }
            }
            if (endIndex <= startIndex || endIndex >= document.Length) return;

            var replacedWordStartIndex = endIndex;
            for (; replacedWordStartIndex < document.Length; replacedWordStartIndex++)
            {
                if (char.IsLetterOrDigit(document[replacedWordStartIndex]))
                {
                    break;
                }
            }

            var replacedWordEndIndex = replacedWordStartIndex;
            for (; replacedWordEndIndex < document.Length; replacedWordEndIndex++)
            {
                if (!char.IsLetterOrDigit(document[replacedWordEndIndex]))
                {
                    break;
                }
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var movingText = document.Substring(startIndex, endIndex - startIndex);
            var replacedWord = document.Substring(replacedWordStartIndex, replacedWordEndIndex - replacedWordStartIndex);
            var selectionMoveAmount = replacedWordEndIndex - endIndex;
            document = document.Remove(replacedWordStartIndex, replacedWordEndIndex - replacedWordStartIndex).Insert(replacedWordStartIndex, movingText)
                .Remove(startIndex, endIndex - startIndex).Insert(startIndex, replacedWord);
            start += selectionMoveAmount;
            end += selectionMoveAmount;

            Document.SetText(TextSetOptions.None, document);
            Document.Selection.SetRange(start, end);

            // After SetText() and SetRange(), RichEdit will scroll selection into view
            // and change scroll viewer's position even if it was already in the viewport
            // It is better to keep its original vertical scroll position after changing the texts' position in this case
            if (wasSelectionInView)
            {
                _contentScrollViewer.ChangeView(
                    null,
                    verticalOffset,
                    zoomFactor: null,
                    disableAnimation: true);
            }
        }
    }
}