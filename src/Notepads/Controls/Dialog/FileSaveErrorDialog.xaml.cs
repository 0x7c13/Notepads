namespace Notepads.Controls.Dialog
{
    using Windows.ApplicationModel.Resources;
    using Windows.UI.Xaml.Controls;

    public sealed partial class FileSaveErrorDialog : ContentDialog, INotepadsDialog
    {
        public bool IsAborted { get; set; }

        public FileSaveErrorDialog(string filePath, string errorMsg)
        {
            InitializeComponent();

            Content = string.IsNullOrEmpty(filePath)
                ? errorMsg
                : string.Format(ResourceLoader.GetForCurrentView().GetString("FileSaveErrorDialog_Content"), filePath, errorMsg);
        }
    }
}
