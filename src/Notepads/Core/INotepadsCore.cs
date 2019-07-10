﻿
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

    public interface INotepadsCore
    {
        event EventHandler<TextEditor> OnActiveTextEditorLoaded;

        event EventHandler<TextEditor> OnActiveTextEditorUnloaded;

        event EventHandler<TextEditor> OnActiveTextEditorSelectionChanged;

        event EventHandler<TextEditor> OnActiveTextEditorEncodingChanged;

        event EventHandler<TextEditor> OnActiveTextEditorLineEndingChanged;

        event KeyEventHandler OnActiveTextEditorKeyDown;

        void CreateNewTextEditor();

        bool TryGetSharingContent(TextEditor textEditor, out string title, out string content);

        bool HaveUnsavedTextEditor();

        void ChangeLineEnding(TextEditor textEditor, LineEnding lineEnding);

        void ChangeEncoding(TextEditor textEditor, Encoding encoding);

        void SwitchTo(bool next);

        TextEditor GetActiveTextEditor();

        void FocusOnActiveTextEditor();

        Task Open(StorageFile file);

        Task<bool> Save(TextEditor textEditor, StorageFile file);
    }
}
