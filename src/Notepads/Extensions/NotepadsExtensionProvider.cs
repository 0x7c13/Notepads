// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Extensions
{
    using Notepads.Controls.Markdown;
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
    }
}