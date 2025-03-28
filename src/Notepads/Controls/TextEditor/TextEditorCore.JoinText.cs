// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Collections.Generic;
    using Windows.UI.Text;
    using Notepads.Services;

    public partial class TextEditorCore
    {
        /// <summary>
        /// Join selected lines into one line using space char as the separator
        /// Example:
        ///   ...
        ///   aaa
        ///   bbb
        ///   ccc
        ///   ...
        ///   ->
        ///   aaa bbb ccc
        /// </summary>
        private void JoinText()
        {
            try
            {
                GetTextSelectionPosition(out var start, out var end);
                GetLineColumnSelection(out var startLine,
                    out var endLine,
                    out var startColumn,
                    out var endColumn,
                    out _,
                    out _);

                // Does not make any sense to join 1 line
                if (startLine == endLine) return;

                var document = GetText();
                var lines = GetDocumentLinesCache();

                var startLineInitialIndex = start - startColumn + 1;
                var endLineFinalIndex = end - endColumn + lines[endLine - 1].Length + 1;
                if (endLineFinalIndex > document.Length) endLineFinalIndex = document.Length;

                if (document[endLineFinalIndex - 1] == RichEditBoxDefaultLineEnding) endLineFinalIndex--;
                if (endLineFinalIndex - startLineInitialIndex <= 0) return;

                var selectedLines = document.Substring(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex);
                var joinedLines = selectedLines.Replace(RichEditBoxDefaultLineEnding, ' ');

                var newContent = document.Remove(startLineInitialIndex, endLineFinalIndex - startLineInitialIndex)
                    .Insert(startLineInitialIndex, joinedLines);

                Document.SetText(TextSetOptions.None, newContent);
                Document.Selection.SetRange(start, end);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(TextEditorCore)}] Failed to join text: {ex}");
                AnalyticsService.TrackEvent("TextEditorCore_FailedToJoinText",
                    new Dictionary<string, string> { { "Exception", ex.ToString() } });
            }
        }
    }
}