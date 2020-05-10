namespace Notepads.Controls.FilePicker
{
    using System.Collections.Generic;
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

        public static FileSavePicker GetFileSavePicker(ITextEditor textEditor, bool saveAs)
        {
            FileSavePicker savePicker = new FileSavePicker
            {
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary
            };

            if (!saveAs && textEditor.EditingFile == null)
            {
                savePicker.DefaultFileExtension = ".txt";
            }
            else if (textEditor.EditingFile != null)
            {
                savePicker.DefaultFileExtension = textEditor.EditingFile.FileType;
            }

            savePicker.FileTypeChoices.Add("Text Documents", FileTypeService.TextDocumentFileExtensions);
            savePicker.FileTypeChoices.Add("All Supported Files", FileTypeService.AllSupportedFileExtensions);
            savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
            savePicker.SuggestedFileName = textEditor.EditingFileName ?? textEditor.FileNamePlaceholder;
            return savePicker;
        }
    }
}