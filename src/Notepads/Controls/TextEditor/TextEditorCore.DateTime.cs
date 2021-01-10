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

        /// <summary>
        /// <para>
        /// Adds "Windows Notepad" style header with current date and time, 
        /// if a text document contains ".LOG" at the very beginning of file.
        /// User can then add log entry from the very next line.
        /// </para>
        /// <see cref="https://support.microsoft.com/help/260563/how-to-use-notepad-to-create-a-log-file"/>
        /// </summary>
        public void TryInsertNewLogEntry()
        {
            if (_hasAddedLogEntry) return;

            var docText = GetText();

            if (!docText.StartsWith(".LOG")) return;

            _hasAddedLogEntry = true;

            Document.Selection.StartPosition = docText.Length;

            Document.Selection.SetText(TextSetOptions.None, RichEditBoxDefaultLineEnding
                + DateTime.Now.ToString("h:mm tt M/dd/yyyy")
                + RichEditBoxDefaultLineEnding);

            Document.Selection.StartPosition = Document.Selection.EndPosition;
        }
    }
}