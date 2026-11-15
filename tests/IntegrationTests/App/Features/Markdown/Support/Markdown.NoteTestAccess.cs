// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Linq;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    internal bool TestRootNoteLoadsCompleted => _notes.Count == _view.GetVisibleNoteEmbeds().Count &&
        _notes.Values.All(note => note.Work.IsCompleted);

    internal string TestRootNoteLoadSummary => string.Join(", ", _notes.Values.Select(note =>
        $"{note.Slot.Target}: work={note.Work.Status}, child={note.Child != null}, attempted={note.Attempted}, " +
        $"admitted={note.Admitted}, rejectedLayout={note.RejectedMountLayout}"));
}
