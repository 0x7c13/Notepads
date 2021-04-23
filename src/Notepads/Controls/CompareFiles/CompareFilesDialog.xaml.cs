using DiffPlex;
using DiffPlex.DiffBuilder;
using DiffPlex.DiffBuilder.Model;
using Notepads.Controls.DiffViewer;
using Notepads.Controls.TextEditor;
using Notepads.Core;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using Windows.ApplicationModel.Resources;
using Windows.Foundation;
using Windows.Foundation.Collections;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Data;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Navigation;

// The Content Dialog item template is documented at https://go.microsoft.com/fwlink/?LinkId=234238

namespace Notepads.Controls.CompareFiles
{
    public sealed partial class CompareFilesDialog : ContentDialog
    {
        private StorageFile file1;
        private StorageFile file2;
        private bool result;
        private readonly ResourceLoader _resourceLoader = ResourceLoader.GetForCurrentView();

        public StorageFile File1 { get => file1; set => file1 = value; }
        public StorageFile File2 { get => file2; set => file2 = value; }
        public bool Result { get => result; set => result = value; }

        public CompareFilesDialog()
        {
            Title = ResourceLoader.GetForCurrentView().GetString("MainMenu_Button_CompareSelector");
            PrimaryButtonText = ResourceLoader.GetForCurrentView().GetString("MainMenu_Button_Compare");
            SecondaryButtonText = ResourceLoader.GetForCurrentView().GetString("MainMenu_Button_Cancel");

            this.InitializeComponent();
        }

        private async void ContentDialog_CompareButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            Result = false;
        }

        private async void ContentDialog_CancelButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
        {
            Result = true;
        }

        public async System.Threading.Tasks.Task<StorageFile> FilePickerAsync()
        {
            FileOpenPicker openPicker = new FileOpenPicker();
            openPicker.ViewMode = PickerViewMode.Thumbnail;
            openPicker.SuggestedStartLocation = PickerLocationId.Desktop;
            openPicker.FileTypeFilter.Add(".txt");
            openPicker.FileTypeFilter.Add("*");

            StorageFile file = await openPicker.PickSingleFileAsync();
            if (file != null)
                return file;
            else
                return null;
        }

        private async void BrowseButton1_Click(object sender, RoutedEventArgs e)
        {
            file1 = await FilePickerAsync();
            if (file1 != null)
                FileNameTextBox1.Text = File1.Path;
            else
                FileNameTextBox1.Text = string.Empty;
        }

        private async void BrowseButton2_Click(object sender, RoutedEventArgs e)
        {
            file2 = await FilePickerAsync();
            if (file2 != null)
                FileNameTextBox2.Text = File2.Path;
            else
                FileNameTextBox2.Text = string.Empty;
        }

        private void FileNameTextBox1_TextChanged(object sender, TextChangedEventArgs e)
        {
            if (FileNameTextBox1.Text.IndexOfAny(Path.GetInvalidFileNameChars()) == 1 && FileNameTextBox2.Text.IndexOfAny(Path.GetInvalidFileNameChars()) == 1)
                IsPrimaryButtonEnabled = true;
            else
                IsPrimaryButtonEnabled = false;
        }

        private void FileNameTextBox2_TextChanged(object sender, TextChangedEventArgs e)
        {
            if (FileNameTextBox1.Text.IndexOfAny(Path.GetInvalidFileNameChars()) == 1 && FileNameTextBox2.Text.IndexOfAny(Path.GetInvalidFileNameChars()) == 1)
                IsPrimaryButtonEnabled = true;
            else
                IsPrimaryButtonEnabled = false;
        }
    }
}
