
namespace Notepads.Extensions
{
    using Notepads.Utilities;

    public interface INotepadsExtensionProvider
    {
        IContentPreviewExtension GetContentPreviewExtension(FileType fileType);

        IContentPreviewExtension GetDiffViewerExtension(FileType fileType);

        IContentExtension[] GetContentExtensions(FileType fileType);
    }
}
