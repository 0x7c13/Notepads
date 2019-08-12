
namespace Notepads.Extensions
{
    using Notepads.Extensions.Markdown;
    using Notepads.Utilities;

    public class NotepadsExtensionProvider : INotepadsExtensionProvider
    {
        public IContentPreviewExtension GetContentPreviewExtension(FileType fileType)
        {
            switch (fileType)
            {
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
