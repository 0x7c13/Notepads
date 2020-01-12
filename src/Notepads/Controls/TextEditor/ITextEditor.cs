namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Text;
    using System.Threading.Tasks;
    using Notepads.Models;
    using Notepads.Utilities;
    using Windows.Storage;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Input;

    public interface ITextEditor
    {
        event RoutedEventHandler Loaded;
        event RoutedEventHandler Unloaded;

        event KeyEventHandler KeyDown;
        event EventHandler ModeChanged;
        event EventHandler ModificationStateChanged;
        event EventHandler FileModificationStateChanged;
        event EventHandler LineEndingChanged;
        event EventHandler EncodingChanged;
        event EventHandler SelectionChanged;
        event EventHandler TextChanging;
        event EventHandler ChangeReverted;
        event EventHandler FileSaved;
        event EventHandler FileReloaded;

        Guid Id { get; set; }

        FileType FileType { get; }

        TextFile LastSavedSnapshot { get; }

        LineEnding? RequestedLineEnding { get; }

        Encoding RequestedEncoding { get; }

        string FileNamePlaceholder { get; set; }

        string EditingFileName { get; }

        string EditingFilePath { get; }

        StorageFile EditingFile { get; }

        bool IsModified { get; }

        FileModificationState FileModificationState { get; }

        TextEditorMode Mode { get; }

        void Init(TextFile textFile,
            StorageFile file,
            bool resetLastSavedSnapshot = true,
            bool clearUndoQueue = true,
            bool isModified = false,
            bool resetText = true);

        string GetText();

        void StartCheckingFileStatusPeriodically();

        void StopCheckingFileStatus();

        TextEditorStateMetaData GetTextEditorStateMetaData();

        void ResetEditorState(TextEditorStateMetaData metadata, string newText = null);

        Task ReloadFromEditingFile();

        LineEnding GetLineEnding();

        Encoding GetEncoding();

        void CopySelectedTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args);

        void RevertAllChanges();

        bool TryChangeEncoding(Encoding encoding);

        bool TryChangeLineEnding(LineEnding lineEnding);

        void ShowHideContentPreview();

        void OpenSideBySideDiffViewer();

        void CloseSideBySideDiffViewer();

        void GetCurrentLineColumn(out int lineIndex, out int columnIndex, out int selectedCount);

        bool IsEditorEnabled();

        Task SaveContentToFileAndUpdateEditorState(StorageFile file);

        string GetContentForSharing();

        void TypeText(string text);

        void Focus();

        bool NoChangesSinceLastSaved(bool compareTextOnly = false);

        void ShowFindAndReplaceControl(bool showReplaceBar);

        void HideFindAndReplaceControl();

        void ShowGoToControl();
      
        void HideGoToControl();

        void Dispose();
    }
}