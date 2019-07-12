
namespace Notepads.Core
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using System.Text;
    using System.Threading.Tasks;
    using Windows.Storage;
    using Windows.UI.Xaml.Input;
    using Notepads.Controls.TextEditor;
    using Notepads.Utilities;

    // INotepadsCore handles Tabs and TextEditor life cycle
    public interface INotepadsCore
    {
        event EventHandler<TextEditor> OnTextEditorLoaded;

        event EventHandler<TextEditor> OnTextEditorUnloaded;

        event EventHandler<TextEditor> OnTextEditorClosingWithUnsavedContent;

        event EventHandler<TextEditor> OnTextEditorSelectionChanged;

        event EventHandler<TextEditor> OnTextEditorEncodingChanged;

        event EventHandler<TextEditor> OnTextEditorLineEndingChanged;

        event KeyEventHandler OnTextEditorKeyDown;

        void OpenNewTextEditor();

        Task OpenNewTextEditor(StorageFile file);

        Task<bool> SaveTextEditorContentToFile(TextEditor textEditor, StorageFile file);

        void DeleteTextEditor(TextEditor textEditor);

        int GetNumberOfOpenedTextEditors();

        bool TryGetSharingContent(TextEditor textEditor, out string title, out string content);

        bool HaveUnsavedTextEditor();

        void ChangeLineEnding(TextEditor textEditor, LineEnding lineEnding);

        void ChangeEncoding(TextEditor textEditor, Encoding encoding);

        void SwitchTo(bool next);

        TextEditor GetSelectedTextEditor();

        void FocusOnTextEditor(TextEditor textEditor);

        void FocusOnSelectedTextEditor();
    }
}
