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
    using Windows.UI.Xaml.Controls.Primitives;
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
        event EventHandler FontZoomFactorChanged;
        event EventHandler TextChanging;
        event EventHandler ChangeReverted;
        event EventHandler FileSaved;
        event EventHandler FileReloaded;
        event EventHandler FileRenamed;

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

        bool DisplayLineNumbers { get; set; }

        bool DisplayLineHighlighter { get; set; }

        void Init(TextFile textFile,
            StorageFile file,
            bool resetLastSavedSnapshot = true,
            bool clearUndoQueue = true,
            bool isModified = false,
            bool resetText = true);

        Task RenameAsync(string newFileName);

        string GetText();

        void StartCheckingFileStatusPeriodically();

        void StopCheckingFileStatus();

        TextEditorStateMetaData GetTextEditorStateMetaData();

        void ResetEditorState(TextEditorStateMetaData metadata, string newText = null);

        Task ReloadFromEditingFileAsync(Encoding encoding = null);

        LineEnding GetLineEnding();

        Encoding GetEncoding();

        void CopyTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args);

        void RevertAllChanges();

        bool TryChangeEncoding(Encoding encoding);

        bool TryChangeLineEnding(LineEnding lineEnding);

        void ShowHideContentPreview();

        void OpenSideBySideDiffViewer();

        void CloseSideBySideDiffViewer();

        /// <summary>
        /// Returns 1-based indexing values
        /// </summary>
        void GetLineColumnSelection(
            out int startLineIndex,
            out int endLineIndex,
            out int startColumnIndex,
            out int endColumnIndex,
            out int selectedCount,
            out int lineCount);

        double GetFontZoomFactor();

        void SetFontZoomFactor(double fontZoomFactor);

        bool IsEditorEnabled();

        Task SaveContentToFileAndUpdateEditorStateAsync(StorageFile file);

        string GetContentForSharing();

        void TypeText(string text);

        void Focus();

        bool NoChangesSinceLastSaved(bool compareTextOnly = false);

        void ShowFindAndReplaceControl(bool showReplaceBar);

        void HideFindAndReplaceControl();

        void ShowGoToControl();

        void HideGoToControl();

        void Dispose();

        FlyoutBase GetContextFlyout();
    }
}