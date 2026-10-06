// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Documents.FileTypes;

namespace Notepads.Presentation.PreviewExtensions;

public interface INotepadsExtensionProvider
{
    IContentPreviewExtension GetContentPreviewExtension(FileType fileType);
}
