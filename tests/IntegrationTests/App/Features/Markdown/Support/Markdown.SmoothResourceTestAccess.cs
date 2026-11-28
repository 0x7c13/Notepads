// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Linq;

namespace Notepads.Presentation.Controls.Markdown;

// Tests/App only: keep the actual host's monotonic counter in sync while
// exercising the real native resource invalidation inside a provider callback.
public sealed partial class MarkdownExtensionView
{
    internal ulong TestInvalidateSmoothImageResources()
    {
        var generation = checked(++_providerGeneration);
        _view.SetImageProvider(null, generation);
        return generation;
    }

    internal void TestRestoreSmoothImageResources() => ReplaceImages();

    internal ulong TestImageProviderGeneration => _providerGeneration;

    internal bool TestHasRejectedRootNoteMount(string target) =>
        _notes.Values.Any(note => note.Slot.Target == target && note.RejectedMountLayout.HasValue);
}
