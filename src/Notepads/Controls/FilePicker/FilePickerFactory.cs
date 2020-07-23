namespace Notepads.Controls.FilePicker
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Notepads.Controls.TextEditor;
    using Notepads.Services;
    using Notepads.Utilities;
    using Windows.Storage.Pickers;

    public static class FilePickerFactory
    {
        private static IList<string> _allSupportedExtensions;

        public static IList<string> AllSupportedExtensions
        {
            get
            {
                if (_allSupportedExtensions == null)
                {
                    var allSupportedExtensions = FileExtensionProvider.AllSupportedFileExtensions.ToList();

                    foreach (var extension in FileExtensionProvider.TextDocumentFileExtensions)
                    {
                        allSupportedExtensions.Remove(extension);
                    }

                    allSupportedExtensions.Sort();
                    allSupportedExtensions.InsertRange(0, FileExtensionProvider.TextDocumentFileExtensions);

                    _allSupportedExtensions = allSupportedExtensions;
                }

                return _allSupportedExtensions;
            }
        }

        public static FileOpenPicker GetFileOpenPicker()
        {
            var fileOpenPicker = new FileOpenPicker
            {
                SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.DocumentsLibrary
            };

            fileOpenPicker.FileTypeFilter.Add("*"); // All files

            foreach (var extension in AllSupportedExtensions)
            {
                fileOpenPicker.FileTypeFilter.Add(extension);
            }

            return fileOpenPicker;
        }

        public static FileSavePicker GetFileSavePicker(ITextEditor textEditor)
        {
            FileSavePicker savePicker = new FileSavePicker
            {
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary
            };

            var fileName = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder;
            var extension = FileTypeUtility.GetFileExtension(fileName).ToLower();

            if (FileExtensionProvider.TextDocumentFileExtensions.Contains(extension, StringComparer.OrdinalIgnoreCase))
            {
                savePicker.FileTypeChoices.Add("Text Documents", FileExtensionProvider.TextDocumentFileExtensions.ToList());
                savePicker.FileTypeChoices.Add("All Supported Files", AllSupportedExtensions);
                savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
            }
            else if (AllSupportedExtensions.Contains(extension, StringComparer.OrdinalIgnoreCase))
            {
                savePicker.FileTypeChoices.Add("All Supported Files", AllSupportedExtensions);
                savePicker.FileTypeChoices.Add("Text Documents", FileExtensionProvider.TextDocumentFileExtensions.ToList());
                savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
            }
            else
            {
                savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
                savePicker.FileTypeChoices.Add("Text Documents", FileExtensionProvider.TextDocumentFileExtensions.ToList());
                savePicker.FileTypeChoices.Add("All Supported Files", AllSupportedExtensions);
            }

            savePicker.SuggestedFileName = fileName;
            return savePicker;
        }
    }
}