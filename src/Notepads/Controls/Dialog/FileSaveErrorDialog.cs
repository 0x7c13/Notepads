namespace Notepads.Controls.Dialog
{
    public sealed class FileSaveErrorDialog : NotepadsDialog
    {
        public FileSaveErrorDialog(string filePath, string errorMsg)
        {
            var content = string.IsNullOrEmpty(filePath) ? errorMsg : string.Format(ResourceLoader.GetString("FileSaveErrorDialog_Content"), filePath, errorMsg);
            Title = ResourceLoader.GetString("FileSaveErrorDialog_Title");
            Content = content;
            PrimaryButtonText = ResourceLoader.GetString("FileSaveErrorDialog_PrimaryButtonText");
        }
    }
}