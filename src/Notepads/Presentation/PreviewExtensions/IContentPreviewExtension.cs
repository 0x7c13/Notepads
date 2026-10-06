// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Presentation.Controls.TextEditor;

namespace Notepads.Presentation.PreviewExtensions;

public interface IContentPreviewExtension : IDisposable
{
    void Bind(TextEditorCore editor);

    bool IsExtensionEnabled { get; set; }
}
