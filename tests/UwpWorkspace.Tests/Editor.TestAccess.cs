// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
namespace Notepads.Presentation.Workspace
{
    public sealed partial class NotepadsCore
    {
        internal int TestOwnedEditorCount => _ownedEditors.Count;
    }
}

namespace Notepads.Presentation.Controls.TextEditor
{
    public sealed partial class TextEditor
    {
        internal TextEditorCore TestNativeCore => TextEditorCore;
    }
}
