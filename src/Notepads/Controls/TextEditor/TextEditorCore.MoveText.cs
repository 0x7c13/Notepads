namespace Notepads.Controls.TextEditor
{
    using System;
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

            MoveLines(document, startLineInitialIndex, endLineFinalIndex, start, end, -lines[startLine - 2].Length - 1);
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

            MoveLines(document, startLineInitialIndex, endLineFinalIndex, start, end, lines[endLine].Length + 1);
        }

        private void MoveLines(string document,
            int startLineInitialIndex, int endLineFinalIndex,
            int selectionStart, int selectionEnd, int selectionMoveAmount)
        {
            var insertIndex = startLineInitialIndex + selectionMoveAmount;
            var movingLines = document.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);
            var remainingContent = document.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);

            if (insertIndex < 0)
            {
                insertIndex = 0;
                remainingContent = RichEditBoxDefaultLineEnding + remainingContent;
                movingLines = movingLines.Remove(0, 1);
            }
            else if (insertIndex >= remainingContent.Length)
            {
                remainingContent += RichEditBoxDefaultLineEnding;
                movingLines = movingLines.Remove(movingLines.Length - 1);
            }

            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var newContent = remainingContent.Insert(insertIndex, movingLines);
            selectionStart += selectionMoveAmount;
            selectionEnd += selectionMoveAmount;
            if (selectionStart < 0) selectionStart = 0;

            Document.SetText(TextSetOptions.None, newContent);
            Document.Selection.SetRange(selectionStart, selectionEnd);

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

            var movingWordIndexData = GetMovingWordsIndexData(document, start, end);
            var startIndex = movingWordIndexData.Item1;
            var endIndex = movingWordIndexData.Item2;
            if (startIndex <= 0 || startIndex >= endIndex) return;
            end = movingWordIndexData.Item3;

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

            MoveWords(document, replacedWordStartIndex, replacedWordEndIndex, startIndex, endIndex, start, end, replacedWordStartIndex - startIndex);
        }

        private void MoveTextRight()
        {
            GetTextSelectionPosition(out var start, out var end);

            var document = GetText();

            if (end >= document.Length) return;

            var movingWordIndexData = GetMovingWordsIndexData(document, start, end);
            var startIndex = movingWordIndexData.Item1;
            var endIndex = movingWordIndexData.Item2;
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

            MoveWords(document, startIndex, endIndex, replacedWordStartIndex, replacedWordEndIndex, start, end, replacedWordEndIndex - endIndex);
        }

        private Tuple<int, int, int> GetMovingWordsIndexData(string document, int selectionStart, int selectionEnd)
        {
            var startIndex = selectionStart;
            if (selectionEnd == selectionStart || char.IsLetterOrDigit(document[selectionStart]))
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

            if (selectionEnd > document.Length) selectionEnd = document.Length;
            var endIndex = selectionEnd;
            if (selectionEnd == selectionStart || char.IsLetterOrDigit(document[selectionEnd - 1]))
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

            return Tuple.Create(startIndex, endIndex, selectionEnd);
        }

        private void MoveWords(string document,
            int leftWordsStartIndex, int leftWordsEndIndex,
            int rightWordsStartIndex, int rightWordsEndIndex,
            int selectionStart, int selectionEnd, int selectionMoveAmount)
        {
            Document.Selection.GetRect(PointOptions.Transform, out Windows.Foundation.Rect rect, out var _);
            GetScrollViewerPosition(out var horizontalOffset, out var verticalOffset);
            var wasSelectionInView = IsSelectionRectInView(rect, horizontalOffset, verticalOffset);

            var leftWords = document.Substring(leftWordsStartIndex, leftWordsEndIndex - leftWordsStartIndex);
            var rightWords = document.Substring(rightWordsStartIndex, rightWordsEndIndex - rightWordsStartIndex);
            document = document.Remove(rightWordsStartIndex, rightWordsEndIndex - rightWordsStartIndex).Insert(rightWordsStartIndex, leftWords)
                .Remove(leftWordsStartIndex, leftWordsEndIndex - leftWordsStartIndex).Insert(leftWordsStartIndex, rightWords);
            selectionStart += selectionMoveAmount;
            selectionEnd += selectionMoveAmount;

            Document.SetText(TextSetOptions.None, document);
            Document.Selection.SetRange(selectionStart, selectionEnd);

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