
namespace Notepads.Extensions
{
    using Notepads.Controls.TextEditor;

    public interface IContentPreviewExtension
    {
        void Bind(TextEditor editor);

        bool IsExtensionEnabled { get; set; }
    }
}