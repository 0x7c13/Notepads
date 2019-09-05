namespace Notepads.Extensions
{
    using Notepads.Utilities;

    public interface INotepadsExtensionProvider
    {
        IContentPreviewExtension GetContentPreviewExtension(FileType fileType);
    }
}