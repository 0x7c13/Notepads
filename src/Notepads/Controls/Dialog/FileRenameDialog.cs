namespace Notepads.Controls.Dialog
{
    using System;
    using System.Collections.Generic;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Notepads.Services;
    using Notepads.Utilities;
    using Microsoft.AppCenter.Analytics;

    public class FileRenameDialog : NotepadsDialog
    {
        private readonly TextBox _fileNameTextBox;

        private readonly TextBlock _errorMessageTextBlock;

        private readonly Action<string> _confirmedAction;

        private readonly string _originalFilename;

        private readonly bool _fileExists;

        public FileRenameDialog(string filename, bool fileExists, Action<string> confirmedAction)
        {
            _originalFilename = filename;
            _fileExists = fileExists;
            _confirmedAction = confirmedAction;

            _fileNameTextBox = new TextBox
            {
                Style = (Style)Application.Current.Resources["TransparentTextBoxStyle"],
                Text = filename,
                IsSpellCheckEnabled = false,
                AcceptsReturn = false,
                SelectionStart = 0,
                SelectionLength = filename.Contains(".") ? filename.LastIndexOf(".", StringComparison.Ordinal) : filename.Length,
                Height = 35,
            };

            _errorMessageTextBlock = new TextBlock()
            {
                Visibility = Visibility.Collapsed,
                Margin = new Thickness(4, 10, 4, 0),
                FontSize = Math.Clamp(_fileNameTextBox.FontSize - 2, 1, Double.PositiveInfinity),
                TextWrapping = TextWrapping.Wrap
            };

            var contentStack = new StackPanel();
            contentStack.Children.Add(_fileNameTextBox);
            contentStack.Children.Add(_errorMessageTextBlock);

            Title = ResourceLoader.GetString("FileRenameDialog_Title");
            Content = contentStack;
            PrimaryButtonText = ResourceLoader.GetString("FileRenameDialog_PrimaryButtonText");
            CloseButtonText = ResourceLoader.GetString("FileRenameDialog_CloseButtonText");
            IsPrimaryButtonEnabled = false;

            _fileNameTextBox.TextChanging += OnTextChanging;
            _fileNameTextBox.KeyDown += OnKeyDown;

            PrimaryButtonClick += (sender, args) => TryRename();

            Analytics.TrackEvent("FileRenameDialogOpened", new Dictionary<string, string>()
            {
                { "FileExists", fileExists.ToString() },
            });
        }

        private bool TryRename()
        {
            var newFileName = _fileNameTextBox.Text;

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

        private void OnKeyDown(object sender, Windows.UI.Xaml.Input.KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter)
            {
                if (TryRename())
                {
                    Hide();
                }
            }
        }

        private void OnTextChanging(TextBox sender, TextBoxTextChangingEventArgs args)
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
                _errorMessageTextBlock.Foreground = new SolidColorBrush(Colors.Red);
                _errorMessageTextBlock.Text = ResourceLoader.GetString($"InvalidFilenameError_{error}");
                _errorMessageTextBlock.Visibility = Visibility.Visible;
            }
            else if (!isExtensionSupported)
            {
                _errorMessageTextBlock.Foreground = new SolidColorBrush(Colors.OrangeRed);
                _errorMessageTextBlock.Text = string.IsNullOrEmpty(fileExtension)
                    ? string.Format(ResourceLoader.GetString("FileRenameError_EmptyFileExtension"))
                    : string.Format(ResourceLoader.GetString("FileRenameError_UnsupportedFileExtension"), fileExtension);
                _errorMessageTextBlock.Visibility = Visibility.Visible;
            }
            else
            {
                _errorMessageTextBlock.Visibility = Visibility.Collapsed;
            }

            IsPrimaryButtonEnabled = isFilenameValid && nameChanged && isExtensionSupported;
        }
    }
}