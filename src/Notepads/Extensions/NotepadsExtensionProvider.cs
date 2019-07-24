
namespace Notepads.Extensions
{
    using Notepads.Utilities;

    public class NotepadsExtensionProvider : INotepadsExtensionProvider
    {
        public IContentPreviewExtension GetContentPreviewExtension(FileType fileType)
        {
            switch (fileType)
            {
                case FileType.TextFile:
                    return new DiffViewer.DiffViewer();
                case FileType.MarkdownFile:
                    return new MarkdownExtensionView();
                default:
                    return null;
            }
        }

        public IContentPreviewExtension GetDiffViewerExtension(FileType fileType)
        {
            return null;
        }

        public IContentExtension[] GetContentExtensions(FileType fileType)
        {
            return null;
        }
    }
}
