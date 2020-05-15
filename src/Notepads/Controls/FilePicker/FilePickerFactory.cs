namespace Notepads.Controls.FilePicker
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Windows.Storage.Pickers;

    public static class FilePickerFactory
    {
        public static FileOpenPicker GetFileOpenPicker()
        {
            var fileOpenPicker = new FileOpenPicker
            {
                SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.DocumentsLibrary
            };
            fileOpenPicker.FileTypeFilter.Add("*");
            return fileOpenPicker;
        }

        public static FileSavePicker GetFileSavePicker(ITextEditor textEditor)
        {
            FileSavePicker savePicker = new FileSavePicker
            {
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary
            };

            var fileName = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder;
            var fileExt = string.Empty;
            if (fileName.Contains("."))
            {
                fileExt = fileName.Split(".").Last();
            }

            if (FileExtensionProvider.TextDocumentFileExtensions.Contains($".{fileExt}", StringComparer.OrdinalIgnoreCase))
            {
                savePicker.FileTypeChoices.Add("Text Documents", FileExtensionProvider.TextDocumentFileExtensions);
                savePicker.FileTypeChoices.Add("All Supported Files", FileExtensionProvider.AllSupportedFileExtensions);
                savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
            }
            else if (FileExtensionProvider.AllSupportedFileExtensions.Contains($".{fileExt}", StringComparer.OrdinalIgnoreCase))
            {
                savePicker.FileTypeChoices.Add("All Supported Files", FileExtensionProvider.AllSupportedFileExtensions);
                savePicker.FileTypeChoices.Add("Text Documents", FileExtensionProvider.TextDocumentFileExtensions);
                savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
            }
            else
            {
                savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
                savePicker.FileTypeChoices.Add("Text Documents", FileExtensionProvider.TextDocumentFileExtensions);
                savePicker.FileTypeChoices.Add("All Supported Files", FileExtensionProvider.AllSupportedFileExtensions);
            }

            savePicker.SuggestedFileName = fileName;
            return savePicker;
        }
    }
}