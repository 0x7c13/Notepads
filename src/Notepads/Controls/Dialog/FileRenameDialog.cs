namespace Notepads.Controls.Dialog
{
    using System;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Text;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;
    using Notepads.Utilities;

    public class FileRenameDialog : NotepadsDialog
    {
        private readonly TextBox _fileNameTextBox;

        private readonly TextBlock _errorMessageTextBlock;

        private readonly Action<string> _confirmedAction;

        public FileRenameDialog(string fileName, Action<string> confirmedAction)
        {
            _confirmedAction = confirmedAction;

            _fileNameTextBox = new TextBox
            {
                Style = (Style)Application.Current.Resources["TransparentTextBoxStyle"],
                IsSpellCheckEnabled = false,
                Text = fileName,
                SelectionStart = 0,
                SelectionLength = fileName.Contains(".") ? fileName.LastIndexOf(".", StringComparison.Ordinal) : fileName.Length,
                Height = 35,
            };

            _errorMessageTextBlock = new TextBlock()
            {
                Visibility = Visibility.Collapsed,
                Margin = new Thickness(4, 10, 4, 0),
                FontSize = Math.Clamp(_fileNameTextBox.FontSize - 2, 1, Double.PositiveInfinity),
                Foreground = new SolidColorBrush(Colors.Red),
                FontStyle = FontStyle.Italic
            };

            var contentStack = new StackPanel();
            contentStack.Children.Add(_fileNameTextBox);
            contentStack.Children.Add(_errorMessageTextBlock);

            Title = "Rename";
            Content = contentStack;
            PrimaryButtonText = "Save";
            CloseButtonText = "Cancel";

            _fileNameTextBox.TextChanging += OnTextChanging;
            _fileNameTextBox.KeyDown += OnKeyDown;

            PrimaryButtonClick += (dialog, args) =>
            {
                var newFileName = _fileNameTextBox.Text;
                if (FileSystemUtility.IsFileNameValid(newFileName, out var error))
                {
                    confirmedAction(newFileName.Trim());
                }
            };
        }

        private void OnKeyDown(object sender, Windows.UI.Xaml.Input.KeyRoutedEventArgs e)
        {
            if (e.Key == VirtualKey.Enter)
            {
                var newFileName = _fileNameTextBox.Text;
                if (FileSystemUtility.IsFileNameValid(newFileName, out _))
                {
                    _confirmedAction(newFileName.Trim());
                    Hide();
                }
            }
        }

        private void OnTextChanging(TextBox sender, TextBoxTextChangingEventArgs args)
        {
            if (args.IsContentChanging)
            {
                var isFileNameValid = FileSystemUtility.IsFileNameValid(sender.Text, out var error);

                IsPrimaryButtonEnabled = isFileNameValid;

                if (!isFileNameValid)
                {
                    _errorMessageTextBlock.Text = error.ToString();
                    _errorMessageTextBlock.Visibility = Visibility.Visible;
                }
                else
                {
                    _errorMessageTextBlock.Visibility = Visibility.Collapsed;
                }
            }
        }
    }
}