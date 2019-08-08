
namespace Notepads.Core
{
    using System;
    using System.Text;
    using System.Threading.Tasks;
    using Notepads.Controls.TextEditor;
    using Notepads.Utilities;
    using Windows.Storage;
    using Windows.UI.Xaml.Input;

    // INotepadsCore handles Tabs and TextEditor life cycle
    public interface INotepadsCore
    {
        event EventHandler<TextEditor> TextEditorLoaded;

        event EventHandler<TextEditor> TextEditorUnloaded;

        event EventHandler<TextEditor> TextEditorEditorModificationStateChanged;

        event EventHandler<TextEditor> TextEditorFileModificationStateChanged;

        event EventHandler<TextEditor> TextEditorSaved;

        event EventHandler<TextEditor> TextEditorClosingWithUnsavedContent;

        event EventHandler<TextEditor> TextEditorSelectionChanged;

        event EventHandler<TextEditor> TextEditorEncodingChanged;

        event EventHandler<TextEditor> TextEditorLineEndingChanged;

        event EventHandler<TextEditor> TextEditorModeChanged;

        event KeyEventHandler TextEditorKeyDown;

        void OpenNewTextEditor();

        Task OpenNewTextEditor(StorageFile file);

        TextEditor OpenNewTextEditor(Guid id, string text, StorageFile file, long dateModifiedFileTime, Encoding encoding, LineEnding lineEnding, bool isModified);

        Task SaveTextEditorContentToFile(TextEditor textEditor, StorageFile file);

        void DeleteTextEditor(TextEditor textEditor);

        int GetNumberOfOpenedTextEditors();

        bool TryGetSharingContent(TextEditor textEditor, out string title, out string content);

        bool HaveUnsavedTextEditor();

        void ChangeLineEnding(TextEditor textEditor, LineEnding lineEnding);

        void ChangeEncoding(TextEditor textEditor, Encoding encoding);

        void SwitchTo(bool next);

        void SwitchTo(TextEditor textEditor);

        TextEditor GetSelectedTextEditor();

        TextEditor[] GetAllTextEditors();

        void FocusOnTextEditor(TextEditor textEditor);

        void FocusOnSelectedTextEditor();

        void CloseTextEditor(TextEditor textEditor);
    }
}
