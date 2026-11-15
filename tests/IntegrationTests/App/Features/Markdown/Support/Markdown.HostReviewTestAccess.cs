// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Windows.System;

namespace Notepads.Presentation.Controls.Markdown
{
    public sealed partial class MarkdownExtensionView
    {
        internal bool TestSessionViewportRestoring => _restoringSessionViewport;
        internal bool TestViewportRestoreDeadlineArmed => _viewportRestoreDeadline != null;
        internal bool TestPreservingPreviewViewport => PreservingPreviewViewport;
        internal Task TestSourceLinkCompletion => SourceLinkCompletion;
        internal void TestApplyPreviewTheme() => ApplyPreviewTheme();
        internal void TestPreviewKeyIntent(VirtualKey key) => ApplyPreviewKeyIntent(key);
        internal void TestBeginSessionViewportRestore()
        {
            CancelSourceLink();
            BeginSessionViewportRestore();
            SessionChanged(this, EventArgs.Empty);
        }
    }
}

namespace Notepads.Presentation.Controls.TextEditor
{
    public sealed partial class TextEditor
    {
        internal bool TestCachedPresentationHidden => _cachedPresentationHidden;
        internal bool TestHandleMarkdownPreviewCommand(VirtualKey key, VirtualKeyModifiers modifiers) =>
            _markdownPreview is { } preview && IsMarkdownPreviewFocused(preview) && HandleMarkdownPreviewCommand(key, modifiers);
    }
}
