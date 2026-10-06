// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
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
    event EventHandler<ITextEditor> TextEditorLoaded;
    event EventHandler<ITextEditor> TextEditorUnloaded;
    event EventHandler<ITextEditor> TextEditorOpened;
    event EventHandler<ITextEditor> TextEditorLanguageChanged;
    event EventHandler<ITextEditor> TextEditorClosed;
    event EventHandler<ITextEditor> TextEditorEditorModificationStateChanged;
    event EventHandler<ITextEditor> TextEditorFileModificationStateChanged;
    event EventHandler<ITextEditor> TextEditorSaved;
    event EventHandler<ITextEditor> TextEditorClosing;
    event EventHandler<ITextEditor> TextEditorRenamed;
    event EventHandler<ITextEditor> TextEditorSelectionChanged;
    event EventHandler<ITextEditor> TextEditorFontZoomFactorChanged;
    event EventHandler<ITextEditor> TextEditorEncodingChanged;
    event EventHandler<ITextEditor> TextEditorLineEndingChanged;
    event EventHandler<ITextEditor> TextEditorModeChanged;
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
        string fileNamePlaceHolder);

    Guid DocumentOwnerId { get; }

    bool IsClosing { get; set; }

    Task<ITextEditor> CreateNewTextEditorAsync(string fileNamePlaceholder);

    void OpenTextEditor(ITextEditor editor, int atIndex = -1);

    void OpenTextEditors(ITextEditor[] editors, Guid? selectedEditorId = null);

    Task SaveContentToFileAndUpdateEditorStateAsync(ITextEditor textEditor, StorageFile file);

    void DeleteTextEditor(ITextEditor textEditor);

    int GetNumberOfOpenedTextEditors();

    bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content);

    bool HaveUnsavedTextEditor();

    bool HaveNonemptyTextEditor();

    void ChangeLineEnding(ITextEditor textEditor, LineEnding lineEnding);

    void SwitchTo(bool next);

    void SwitchTo(int index);

    void SwitchTo(ITextEditor textEditor);

    ITextEditor GetSelectedTextEditor();

    ITextEditor GetTextEditor(StorageFile file);

    ITextEditor[] GetAllTextEditors();

    void FocusOnTextEditor(ITextEditor textEditor);

    void FocusOnSelectedTextEditor();

    void CloseTextEditor(ITextEditor textEditor);

    double GetTabScrollViewerHorizontalOffset();

    void SetTabScrollViewerHorizontalOffset(double offset);
}
