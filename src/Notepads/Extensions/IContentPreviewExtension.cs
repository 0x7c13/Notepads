namespace Notepads.Extensions
{
    using Notepads.Controls.TextEditor;

    public interface IContentPreviewExtension
    {
        void Bind(TextEditorCore editor);

        bool IsExtensionEnabled { get; set; }

        void Dispose();
    }
}