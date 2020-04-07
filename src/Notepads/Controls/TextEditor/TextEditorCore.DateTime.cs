namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Globalization;
    using Windows.UI.Text;

    public partial class TextEditorCore
    {
        private void InsertDateTimeString()
        {
            var dateStr = DateTime.Now.ToString(CultureInfo.CurrentCulture);
            Document.Selection.SetText(TextSetOptions.None, dateStr);
            Document.Selection.StartPosition = Document.Selection.EndPosition;
        }
    }
}