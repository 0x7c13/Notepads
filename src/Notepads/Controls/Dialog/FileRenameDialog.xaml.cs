namespace Notepads.Controls.Dialog
{
    using Microsoft.AppCenter.Analytics;
    using Notepads.Services;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using Windows.ApplicationModel.Resources;
    using Windows.System;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public sealed partial class FileRenameDialog : ContentDialog, INotepadsDialog
    {
        public bool IsAborted { get; set; }

        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();
        private readonly string _originalFilename;
        private readonly bool _fileExists;
        private readonly Action<string> _confirmedAction;

        public FileRenameDialog(string filename, bool fileExists, Action<string> confirmedAction)
        {
            InitializeComponent();

            FileNameBox.Text = filename;
            FileNameBox.SelectionStart = 0;
            FileNameBox.SelectionLength = filename.Contains(".") ? filename.LastIndexOf(".", StringComparison.Ordinal) : filename.Length;

            ErrorMessageBlock.FontSize = Math.Clamp(FileNameBox.FontSize - 2, 1, Double.PositiveInfinity);

            _originalFilename = filename;
            _fileExists = fileExists;
            _confirmedAction = confirmedAction;

            Analytics.TrackEvent("FileRenameDialogOpened", new Dictionary<string, string>()
            {
                { "FileExists", fileExists.ToString() },
            });
        }

        private bool TryRename()
        {
            var newFileName = FileNameBox.Text;

            if (string.Compare(_originalFilename, newFileName, StringComparison.OrdinalIgnoreCase) == 0)
            {
                return false;
            }

            if (!FileSystemUtility.IsFilenameValid(newFileName, out var error))
            {
                return false;
            }

            if (_fileExists && !FileExtensionProvider.IsFileExtensionSupported(FileTypeUtility.GetFileExtension(newFileName)))
            {
                return false;
            }

            _confirmedAction(newFileName.Trim());
            return true;
        }

        private void FileRenameDialog_PrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            TryRename();
        }

        private void FileNameBox_TextChanging(TextBox sender, TextBoxTextChangingEventArgs args)
        {
            if (!args.IsContentChanging) return;

            var newFilename = sender.Text;
            var isFilenameValid = FileSystemUtility.IsFilenameValid(newFilename, out var error);
            var nameChanged = string.Compare(_originalFilename, newFilename, StringComparison.OrdinalIgnoreCase) != 0;
            var isExtensionSupported = false;
            var fileExtension = FileTypeUtility.GetFileExtension(newFilename);

            if (!_fileExists) // User can rename whatever they want for new file
            {
                isExtensionSupported = true;
            }
            else if (FileExtensionProvider.IsFileExtensionSupported(fileExtension))
            {
                // User can only rename an existing file if extension is supported by the app
                // This is a Windows 10 UWP limitation
                isExtensionSupported = true;
            }

            if (!isFilenameValid)
            {
                //ErrorMessageBlock.Foreground = new SolidColorBrush(Colors.Red);
                ErrorMessageBlock.Text = _resourceLoader.GetString($"InvalidFilenameError_{error}");
                ErrorMessageBlock.Visibility = Visibility.Visible;
            }
            else if (!isExtensionSupported)
            {
                //ErrorMessageBlock.Foreground = new SolidColorBrush(Colors.OrangeRed);
                ErrorMessageBlock.Text = string.IsNullOrEmpty(fileExtension)
                    ? string.Format(_resourceLoader.GetString("FileRenameError_EmptyFileExtension"))
                    : string.Format(_resourceLoader.GetString("FileRenameError_UnsupportedFileExtension"), fileExtension);
                ErrorMessageBlock.Visibility = Visibility.Visible;
            }
            else
            {
                ErrorMessageBlock.Visibility = Visibility.Collapsed;
            }

            IsPrimaryButtonEnabled = isFilenameValid && nameChanged && isExtensionSupported;
        }

        private void FileNameBox_KeyDown(object sender, KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter)
            {
                if (TryRename())
                {
                    Hide();
                }
            }
        }
    }
}
