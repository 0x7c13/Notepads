// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2020-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.IO;
using Notepads.Infrastructure.Diagnostics;
using Windows.System;
using Windows.UI;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Dialog;

public sealed partial class FileRenameDialog : NotepadsDialog
{
    private readonly TextBox _fileNameTextBox;

    private readonly TextBlock _errorMessageTextBlock;

    private readonly Action<string> _confirmedAction;

    private readonly string _originalFilename;

    private readonly bool _fileExists;

    private readonly bool _allowUnchangedName;

    public FileRenameDialog(string filename, bool fileExists, Action<string> confirmedAction, bool allowUnchangedName = false)
    {
        _originalFilename = filename;
        _fileExists = fileExists;
        _confirmedAction = confirmedAction;
        // Confirming an automatic draft's displayed title explicitly pins that name.
        _allowUnchangedName = allowUnchangedName && !fileExists;

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
        UpdateValidation();

        _fileNameTextBox.TextChanging += OnTextChanging;
        _fileNameTextBox.KeyDown += OnKeyDown;

        PrimaryButtonClick += (sender, args) => TryRename();

        AnalyticsService.TrackEvent("FileRenameDialogOpened", new Dictionary<string, string>()
        {
            { "FileExists", fileExists.ToString() },
        });
    }

    private bool TryRename()
    {
        UpdateValidation();
        if (!IsPrimaryButtonEnabled) return false;
        _confirmedAction(_fileNameTextBox.Text.Trim());
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
        if (args.IsContentChanging) UpdateValidation();
    }

    private void UpdateValidation()
    {
        var newFilename = _fileNameTextBox.Text;
        var isFilenameValid = FileNameRules.IsFilenameValid(newFilename, out var error);
        var nameChanged = _allowUnchangedName || string.Compare(_originalFilename, newFilename, StringComparison.OrdinalIgnoreCase) != 0;
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
