
namespace Notepads.Controls.TextEditor
{
    using System;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.UI.Xaml;
    using Notepads.Utilities;

    public interface ITextEditor
    {
        event EventHandler ModeChanged;
        event EventHandler ModificationStateChanged;
        event EventHandler FileModificationStateChanged;
        event EventHandler LineEndingChanged;
        event EventHandler EncodingChanged;
        event EventHandler TextChanging;
        event EventHandler ChangeReverted;
        event RoutedEventHandler SelectionChanged;

        Guid Id { get; set; }

        FileType FileType { get; }

        TextFile LastSavedSnapshot { get; }

        LineEnding? RequestedLineEnding { get; }

        Encoding RequestedEncoding { get; }

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

        LineEnding GetLineEnding();

        Encoding GetEncoding();

        void RevertAllChanges();

        bool TryChangeEncoding(Encoding encoding);

        bool TryChangeLineEnding(LineEnding lineEnding);

        void ShowHideContentPreview();

        void OpenSideBySideDiffViewer();

        void CloseSideBySideDiffViewer();

        void GetCurrentLineColumn(out int lineIndex, out int columnIndex, out int selectedCount);

        bool IsEditorEnabled();

        Task SaveContentToFileAndUpdateEditorState(StorageFile file);

        Task<TextFile> SaveContentToFile(StorageFile file);

        string GetContentForSharing();

        void TypeText(string text);

        void Focus();

        bool NoChangesSinceLastSaved(bool compareTextOnly = false);

        void ShowFindAndReplaceControl(bool showReplaceBar);

        void HideFindAndReplaceControl();
    }
}