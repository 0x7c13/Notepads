
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
        event EventHandler<TextEditor> OnTextEditorLoaded;

        event EventHandler<TextEditor> OnTextEditorUnloaded;

        event EventHandler<TextEditor> OnTextEditorSaved;

        event EventHandler<TextEditor> OnTextEditorClosingWithUnsavedContent;

        event EventHandler<TextEditor> OnTextEditorSelectionChanged;

        event EventHandler<TextEditor> OnTextEditorEncodingChanged;

        event EventHandler<TextEditor> OnTextEditorLineEndingChanged;

        event KeyEventHandler OnTextEditorKeyDown;

        void OpenNewTextEditor();

        Task OpenNewTextEditor(StorageFile file);

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
