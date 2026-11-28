// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    private TaskCompletionSource<bool> _testCaretMapReady;
    private TaskCompletionSource<bool> _testCaretMapRelease;
    private Action _testBeforeCaretReveal;

    internal void TestBeforeNextSourceCaretReveal(Action action) => _testBeforeCaretReveal = action;

    internal Task TestHoldNextSourceCaret(out Action release)
    {
        if (_testCaretMapReady != null) throw new InvalidOperationException("A source caret is already held.");
        _testCaretMapReady = new(TaskCreationOptions.RunContinuationsAsynchronously);
        _testCaretMapRelease = new(TaskCreationOptions.RunContinuationsAsynchronously);
        var gate = _testCaretMapRelease;
        release = () => gate.TrySetResult(true);
        return _testCaretMapReady.Task;
    }

    internal void TestQueueSourceCaret()
    {
        if (LinkPreviewPane || !EnsureLinkContext(out var context))
            throw new InvalidOperationException("The current source pane is unavailable.");
        CaptureLinkCaret(context, false, true);
        WakeSourceLink();
    }

    partial void OnLinkCaretMapped(MarkdownFollowWork work, ref Task publicationWait)
    {
        if (work.Pane != MarkdownFollowPane.Source || work.SearchMatch.HasValue) return;
        if (_testBeforeCaretReveal != null && _linkCoordinator.CanRevealCaret(work))
        {
            var action = _testBeforeCaretReveal;
            _testBeforeCaretReveal = null;
            action();
        }
        if (_testCaretMapReady == null) return;
        publicationWait = _testCaretMapRelease.Task;
        var ready = _testCaretMapReady;
        _testCaretMapReady = null;
        _testCaretMapRelease = null;
        ready.TrySetResult(true);
    }

    internal bool TestSourceFindBusy => !_linkCompletion.IsCompleted || _linkCoordinator.HasPending || _linkRendering;
    internal bool TestOwnsSourceMatch => _linkOwnedMatch.HasValue;
    internal void TestBeginViewportScroll(bool preview) => SetLinkPane(preview, LinkIntent.Scroll);
}
