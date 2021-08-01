namespace Notepads.Controls.Dialog
{
    using Windows.ApplicationModel.Resources;
    using Windows.UI.Xaml.Controls;

    public sealed partial class FileOpenErrorDialog : ContentDialog, INotepadsDialog
    {
        public bool IsAborted { get; set; }

        public FileOpenErrorDialog(string filePath, string errorMsg)
        {
            InitializeComponent();

            Content = string.IsNullOrEmpty(filePath)
                ? errorMsg
                : string.Format(ResourceLoader.GetForCurrentView().GetString("FileOpenErrorDialog_Content"), filePath, errorMsg);
        }
    }
}
