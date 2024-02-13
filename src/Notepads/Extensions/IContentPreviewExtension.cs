// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Extensions
{
    using System;
    using Controls.TextEditor;

    public interface IContentPreviewExtension : IDisposable
    {
        void Bind(TextEditorCore editor);

        bool IsExtensionEnabled { get; set; }
    }
}