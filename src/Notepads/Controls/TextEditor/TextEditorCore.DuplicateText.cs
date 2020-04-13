namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using Windows.UI.Text;
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;

    public partial class TextEditorCore
    {
        private void DuplicateText()
        {
            try
            {
                GetLineColumnSelection(
                    out int startLineIndex,
                    out int endLineIndex,
                    out int startColumnIndex,
                    out int endColumnIndex,
                    out int selectedCount,
                    out int lineCount);

                GetTextSelectionPosition(out var start, out var end);

                if (end == start)
                {
                    // Duplicate Line
                    var line = _contentLinesCache[startLineIndex - 1];
                    var column = Document.Selection.EndPosition + line.Length + 1;

                    if (startColumnIndex == 1)
                        Document.Selection.EndPosition += 1;

                    Document.Selection.EndOf(TextRangeUnit.Paragraph, false);

                    if (startLineIndex < lineCount)
                        Document.Selection.EndPosition -= 1;

                    Document.Selection.SetText(TextSetOptions.None, RichEditBoxDefaultLineEnding + line);
                    Document.Selection.StartPosition = Document.Selection.EndPosition = column;
                }
                else
                {
                    // Duplicate selection
                    var textRange = Document.GetRange(start, end);
                    textRange.GetText(TextGetOptions.None, out string text);

                    if (text.EndsWith(RichEditBoxDefaultLineEnding))
                    {
                        Document.Selection.EndOf(TextRangeUnit.Line, false);

                        if (startLineIndex < lineCount && end < _content.Length)
                            Document.Selection.StartPosition = Document.Selection.EndPosition - 1;
                    }
                    else
                    {
                        Document.Selection.StartPosition = Document.Selection.EndPosition;
                    }

                    Document.Selection.SetText(TextSetOptions.None, text);
                }
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[TextEditorCore] Failed to duplicate text: {ex}");
                Analytics.TrackEvent("TextEditorCore_FailedToDuplicateText",
                    new Dictionary<string, string> { { "Exception", ex.ToString() } });
            }
        }
    }
}