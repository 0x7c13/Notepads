// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Workspace;

/// <summary>
/// INotepadsCore handles Tabs and TextEditor life cycle
/// </summary>
public interface INotepadsCore : IDisposable, IAsyncDisposable
{
    event EventHandler<WorkspaceTab> TabOpened;
    event EventHandler<WorkspaceTab> TabClosed;
    event EventHandler<WorkspaceTab> TabClosing;
    event EventHandler<WorkspaceTab> TabSelected;
    event EventHandler<WorkspaceTab> TabStateChanged;
    event EventHandler<WorkspaceTab> EditorAttached;
    event EventHandler<ITextEditor> TextEditorLoaded;
    event EventHandler<ITextEditor> TextEditorLanguageChanged;
    event EventHandler<ITextEditor> TextEditorEditorModificationStateChanged;
    event EventHandler<ITextEditor> TextEditorFileModificationStateChanged;
    event EventHandler<ITextEditor> TextEditorSaved;
    event EventHandler<ITextEditor> TextEditorRenamed;
    event EventHandler<ITextEditor> TextEditorSelectionChanged;
    event EventHandler<ITextEditor> TextEditorFontZoomFactorChanged;
    event EventHandler<ITextEditor> TextEditorEncodingChanged;
    event EventHandler<ITextEditor> TextEditorLineEndingChanged;
    event EventHandler<ITextEditor> TextEditorMovedToAnotherAppInstance;
    event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;
    event KeyEventHandler TextEditorKeyDown;

    Task<ITextEditor> CreateTextEditorAsync(
        Guid id,
        StorageFile file,
        Encoding encoding = null);

    ITextEditor CreateTextEditor(
        Guid id,
        StorageFile editingFile,
        string fileNamePlaceHolder,
        bool allowWhileClosing = false);

    Guid DocumentOwnerId { get; }

    bool IsClosing { get; set; }

    bool IsEditorPresentationPaused { get; set; }

    Task<ITextEditor> CreateNewTextEditorAsync(string fileNamePlaceholder);

    Task DiscardUnattachedEditorAsync(ITextEditor editor);

    void OpenTextEditor(ITextEditor editor, int atIndex = -1);

    void OpenTab(WorkspaceTab tab, int atIndex = -1, bool select = true, bool allowWhileClosing = false);

    void OpenTabs(WorkspaceTab[] tabs, Guid? selectedTabId = null, bool allowWhileClosing = false);

    bool AttachEditor(WorkspaceTab tab, ITextEditor editor, bool showEditor = true);

    void ResumeEditorPresentation();

    WorkspaceTab[] GetTabs();

    WorkspaceTab GetSelectedTab();

    WorkspaceTab GetTab(ITextEditor editor);

    bool ContainsTab(WorkspaceTab tab);

    WorkspaceTab FindReadyTab(StorageFile file);

    Task<bool> OpenFileAsync(StorageFile file, Func<WorkspaceFileOpenResult, Task> presentResultAsync);

    void DeleteTab(WorkspaceTab tab);

    void CloseTab(WorkspaceTab tab);

    void SwitchTo(WorkspaceTab tab);

    Task SaveContentToFileAndUpdateEditorStateAsync(ITextEditor textEditor, StorageFile file);

    int GetTabCount();

    bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content);

    bool HasTabsRequiringSave();

    bool HasPrintableTabs();

    void ChangeLineEnding(ITextEditor textEditor, LineEnding lineEnding);

    void SwitchTo(bool next);

    void SwitchTo(int index);

    void SwitchTo(ITextEditor textEditor);

    ITextEditor GetSelectedReadyEditor();

    void FocusOnTextEditor(ITextEditor textEditor);

    void FocusOnSelectedTextEditor();

    double GetTabScrollViewerHorizontalOffset();

    void SetTabScrollViewerHorizontalOffset(double offset);
}
