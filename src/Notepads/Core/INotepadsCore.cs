
namespace Notepads.Core
{
    using Notepads.Controls.TextEditor;
    using Notepads.Utilities;
    using System;
    using System.Collections.Generic;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.UI.Xaml.Input;

    // INotepadsCore handles Tabs and TextEditor life cycle
    public interface INotepadsCore
    {
        event EventHandler<ITextEditor> TextEditorLoaded;

        event EventHandler<ITextEditor> TextEditorUnloaded;

        event EventHandler<ITextEditor> TextEditorEditorModificationStateChanged;

        event EventHandler<ITextEditor> TextEditorFileModificationStateChanged;

        event EventHandler<ITextEditor> TextEditorSaved;

        event EventHandler<ITextEditor> TextEditorClosingWithUnsavedContent;

        event EventHandler<ITextEditor> TextEditorSelectionChanged;

        event EventHandler<ITextEditor> TextEditorEncodingChanged;

        event EventHandler<ITextEditor> TextEditorLineEndingChanged;

        event EventHandler<ITextEditor> TextEditorModeChanged;

        event EventHandler<IReadOnlyList<IStorageItem>> StorageItemsDropped;

        event KeyEventHandler TextEditorKeyDown;

        Task<ITextEditor> CreateTextEditor(
            Guid id,
            StorageFile file,
            bool ignoreFileSizeLimit = false);

        ITextEditor CreateTextEditor(
            Guid id,
            TextFile textFile,
            StorageFile editingFile,
            bool isModified);

        void OpenNewTextEditor();

        void OpenTextEditor(ITextEditor editor, int atIndex = -1);

        void OpenTextEditors(ITextEditor[] editors, Guid? selectedEditorId = null);

        Task SaveContentToFileAndUpdateEditorState(ITextEditor textEditor, StorageFile file);

        void DeleteTextEditor(ITextEditor textEditor);

        int GetNumberOfOpenedTextEditors();

        bool TryGetSharingContent(ITextEditor textEditor, out string title, out string content);

        bool HaveUnsavedTextEditor();

        void ChangeLineEnding(ITextEditor textEditor, LineEnding lineEnding);

        void ChangeEncoding(ITextEditor textEditor, Encoding encoding);

        void SwitchTo(bool next);

        void SwitchTo(ITextEditor textEditor);

        ITextEditor GetSelectedTextEditor();

        ITextEditor GetTextEditor(StorageFile file);

        ITextEditor GetTextEditor(string editingFilePath);

        ITextEditor[] GetAllTextEditors();

        void FocusOnTextEditor(ITextEditor textEditor);

        void FocusOnSelectedTextEditor();

        void CloseTextEditor(ITextEditor textEditor);
    }
}
