// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Input;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

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
    event EventHandler LanguageChanged;
    event EventHandler LanguageOverrideChanged;

    DocumentLanguage DocumentLanguage { get; }
    DocumentLanguage DetectedLanguage { get; }
    string LanguageOverride { get; }
    int SyntaxHighlightingPauseReason { get; }
    bool CanChangeLanguage { get; }
    void SetLanguageOverride(string id);

    Guid Id { get; set; }

    FileType FileType { get; }

    DocumentSnapshot LastSavedSnapshot { get; }

    Guid DocumentOwnerId { get; set; }

    LineEnding? RequestedLineEnding { get; }

    Encoding RequestedEncoding { get; }

    string FileNamePlaceholder { get; set; }

    string EditingFileName { get; }

    string EditingFilePath { get; }

    StorageFile EditingFile { get; }

    bool IsModified { get; }

    bool IsDocumentEmpty { get; }

    FileModificationState FileModificationState { get; }

    TextEditorMode Mode { get; }

    bool DisplayLineNumbers { get; set; }

    bool DisplayLineHighlighter { get; set; }

    void InitEmpty(Encoding encoding, LineEnding lineEnding, StorageFile file = null);

    Task InitializeNewAsync();

    ulong DocumentSequence { get; }

    DocumentRecoveryState CaptureRecoveryState();

    Task MaintainRecoveryAsync(CancellationToken cancellationToken = default);

    Task WaitForDocumentOperationsAsync();

    Task<bool> TryCommitTransferAsync(Func<bool> sourceUnchanged,
        Func<Task> commitRemovalIntent, Action removeLogicalSource);

    Task InitAsync(DocumentSnapshot snapshot, StorageFile file, bool isModified = false);

    Task RestoreAsync(DocumentSnapshot snapshot, StorageFile file, DocumentMetadata metadata,
        DocumentBaseline pendingBaseline = null);

    Task RestoreV2Async(DocumentSnapshot savedSnapshot, DocumentBaseline recoveryBaseline,
        DocumentJournal journal, EditorJournalCheckpoint checkpoint, StorageFile file,
        DocumentMetadata metadata, bool textDirty);

    Task RenameAsync(string newFileName);

    Task RenameAsync(string newFileName, StorageFile expectedFile);

    string GetText();

    Task<string> GetTextAsync();

    void StartCheckingFileStatusPeriodically();

    void StopCheckingFileStatus();

    DocumentMetadata GetTextEditorStateMetaData();

    void ResetEditorState(DocumentMetadata metadata);

    Task ReloadFromEditingFileAsync(Encoding encoding = null,
        DocumentDecodingMode decodingMode = DocumentDecodingMode.ConfiguredDefault);

    LineEnding GetLineEnding();

    Encoding GetEncoding();

    void CopyTextToWindowsClipboard(TextControlCopyingToClipboardEventArgs args);

    Task RevertAllChangesAsync();

    bool TryChangeEncoding(Encoding encoding);

    bool TryChangeLineEnding(LineEnding lineEnding);

    void ShowHideContentPreview();

    Task OpenSideBySideDiffViewerAsync();

    Task ToggleDiffPreviewAsync();

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

    /// <summary>Completes only after disposal has drained document work and native journal writers.</summary>
    Task DisposalCompletion { get; }

    FlyoutBase GetContextFlyout();
}
