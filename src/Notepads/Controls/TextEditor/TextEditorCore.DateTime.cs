namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Globalization;
    using Windows.UI.Text;

    public partial class TextEditorCore
    {
        private bool _hasAddedLogEntry = false;

        private void InsertDateTimeString()
        {
            var dateStr = DateTime.Now.ToString(CultureInfo.CurrentCulture);
            Document.Selection.SetText(TextSetOptions.None, dateStr);
            Document.Selection.StartPosition = Document.Selection.EndPosition;
        }

        public void TryInsertNewLogEntry()
        {
            var docText = GetText();
            if (!docText.StartsWith(".LOG") || _hasAddedLogEntry) return;

            _hasAddedLogEntry = true;
            Document.Selection.StartPosition = docText.Length;

            Document.Selection.SetText(TextSetOptions.None, RichEditBoxDefaultLineEnding
                + DateTime.Now.ToString("h:mm tt M/dd/yyyy")
                + RichEditBoxDefaultLineEnding);

            Document.Selection.StartPosition = Document.Selection.EndPosition;
        }
    }
}