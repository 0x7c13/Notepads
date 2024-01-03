namespace Notepads.Controls.Dialog
{
    public sealed class FileOpenErrorDialog : NotepadsDialog
    {
        public FileOpenErrorDialog(string filePath, string errorMsg)
        {
            Title = ResourceLoader.GetString("FileOpenErrorDialog_Title");
            Content = string.IsNullOrEmpty(filePath) ? errorMsg : string.Format(ResourceLoader.GetString("FileOpenErrorDialog_Content"), filePath, errorMsg);
            PrimaryButtonText = ResourceLoader.GetString("FileOpenErrorDialog_PrimaryButtonText");
        }
    }
}