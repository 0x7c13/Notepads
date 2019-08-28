
namespace Notepads.Services
{
    using Notepads.Controls.TextEditor;
    using System.Collections.Generic;
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

        public static FileSavePicker GetFileSavePicker(ITextEditor textEditor, string defaultFileName, bool saveAs)
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

            savePicker.FileTypeChoices.Add("Text Documents", new List<string>() { ".txt", ".md", });
            savePicker.FileTypeChoices.Add("All Supported Files", new List<string>()
            {
                ".txt", ".md",
                ".cfg", ".config", ".cnf", ".conf", ".ini", ".log",
                ".json", ".yml", ".yaml", ".xml", ".xaml",
                ".html", ".htm", ".asp", ".aspx", ".jsp", ".jspx", ".css", ".scss",
                ".ps1", ".bat", ".cmd", ".vbs", ".sh", ".bashrc", ".rc", ".bash",
                ".c", ".cmake", ".h", ".hpp", ".cpp", ".cc", ".cs", ".m", ".mm", ".php", ".py", ".rb", ".vb", ".java",
                ".js", ".ts", ".lua",
            });
            savePicker.FileTypeChoices.Add("Unknown", new List<string>() { "." });
            savePicker.SuggestedFileName = textEditor.EditingFileName ?? defaultFileName;
            return savePicker;
        }
    }
}
