// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Composition;
using Notepads.Controls;
using Notepads.Features.Sessions;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Recovery;
using Notepads.Features.Sessions.Storage;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Views.MainPage;
using Notepads.Presentation.Workspace;
using NotepadsEditorTests;
using Windows.Foundation;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class WorkspaceStartupTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var owner = Guid.NewGuid();
        try
        {
            var ids = await SeedAsync(owner);
            await VerifyPresentationAsync(owner, ids);
        }
        finally { await SessionTestProtocol.CleanupAsync(owner); }
        log.AppendLine("PASS: selected middle tab first paint keeps viewport, caret and tab-strip offset.");
        log.AppendLine("PASS: reentrant scroll and explicit tab reveal replace pending offsets across layout and reattachment.");
        log.AppendLine("PASS: next/previous and numbered tab commands supersede pending offset/reveal requests.");
        log.AppendLine("PASS: control-level header selection keeps the selected tab visible after pending scrolling.");
    }

    private static SessionService CreateService(Guid owner) => new(
        new SessionScopeData { OwnerId = owner, InstanceId = owner, Kind = SessionScopeData.Secondary },
        SessionScopeData.GetSecondaryPrefix(owner) + "NotepadsSessionData.json", () => new[] { owner });

    private static async Task<Guid[]> SeedAsync(Guid owner)
    {
        await using var service = CreateService(owner);
        service.IsBackupEnabled = true;
        await service.InitializeAuthorityAsync();
        var ids = Enumerable.Range(0, 8).Select(_ => Guid.NewGuid()).ToArray();
        var documents = new List<SessionTestDocument>();
        try
        {
            foreach (var id in ids)
            {
                var text = string.Concat(Enumerable.Range(0, 160).Select(line => $"{id:N} line {line} 中文😀\r"));
                var document = await SessionTestDocument.CreateAsync(owner, text);
                documents.Add(document);
                document.Metadata.FileNamePlaceholder = id.ToString("N") + ".txt";
                document.Metadata.FirstVisibleDocumentLine = 80;
                document.Metadata.FontZoomFactor = 1;
                document.Metadata.WrapWord = false;
                document.Metadata.SelectionStartPosition = document.Metadata.SelectionEndPosition = 40;
            }
            using var capture = SessionTestProtocol.CaptureDocuments(documents, ids, service.CurrentStamp);
            capture.SelectedEditorId = ids[3];
            capture.TabScrollOffset = 120;
            Check((await service.SaveAsync(capture, CancellationToken.None)).Succeeded, "Could not seed startup workspace.");
        }
        finally { foreach (var document in documents) await document.DisposeAsync(); }
        return ids;
    }

    private static (Frame Frame, NotepadsMainPage Page) CreatePage(Guid owner, SessionService service)
    {
        var registry = new SessionRegistry();
        var context = new WindowContext("Startup Tests", owner, false, owner, Window.Current.Dispatcher,
            service, registry.Register, () => { });
        var frame = new Frame();
        Check(frame.Navigate(typeof(NotepadsMainPage), new WorkspaceNavigation(context, null)), "Startup page navigation failed.");
        return (frame, (NotepadsMainPage)frame.Content);
    }

    private static async Task VerifyPresentationAsync(Guid owner, Guid[] ids)
    {
        var seeded = await SessionRecoveryCatalog.ReadScopeAsync(owner);
        Check(seeded.Selection.TabScrollViewerHorizontalOffset == 120, "The startup fixture must persist its tab-strip offset.");
        await using var service = CreateService(owner);
        var (frame, page) = CreatePage(owner, service);
        WinUIEditor.Editor observed = null;
        var painted = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        var sets = (SetsView)page.FindName("Sets");
        sets.Width = 600;
        var core = page.TestCore;
        void OnPainted(WinUIEditor.Editor sender, WinUIEditor.PaintedEventArgs args)
        {
            sender.Painted -= OnPainted;
            var scroller = Descendants(sets).OfType<ScrollViewer>().First(item => item.Name == "ScrollViewer");
            var correct = sender.FirstVisibleLine == 80 && sender.CurrentPos == 40 &&
                core.GetSelectedTab()?.Id == ids[3] && Math.Abs(scroller.HorizontalOffset - 120) < 1;
            painted.TrySetResult(correct ? null : $"line={sender.FirstVisibleLine}, caret={sender.CurrentPos}, " +
                $"selected={Array.IndexOf(ids, core.GetSelectedTab()?.Id ?? Guid.Empty)}, tabOffset={scroller.HorizontalOffset}, " +
                $"requested={core.GetTabScrollViewerHorizontalOffset()}, max={scroller.ScrollableWidth}, " +
                $"extent={scroller.ExtentWidth}, viewport={scroller.ViewportWidth}, tabs={core.GetTabCount()}");
        }
        // Observe the restored editor before it is attached to the live visual tree.
        page.TestController.BeforeRestorePromotion = (source, editor, cancellation) =>
        {
            if (source.Id == ids[3])
            {
                var nativeCore = ((TextEditor)editor).TestNativeCore;
                observed = ((WinUIEditor.EditorBaseControl)((Grid)nativeCore.Content).Children[0]).Editor;
                observed.Painted += OnPainted;
            }
            return Task.CompletedTask;
        };
        try
        {
            Window.Current.Content = frame;
            Window.Current.Activate();
            var failure = await painted.Task.WaitAsync(TimeSpan.FromSeconds(15));
            Check(failure == null, "First native paint lost viewport, selection or tab-strip position: " + failure);
            await UntilAsync(() => page.TestStartupComplete, "Startup did not complete.");
            page.TestController.StopSessionBackup();
            Check(core.GetTabCount() == ids.Length && core.GetSelectedReadyEditor() != null,
                "Startup did not restore its roster and selected document.");
            Check(await page.TestController.SaveSessionAsync(), "Restored workspace checkpoint failed.");
            var saved = await SessionRecoveryCatalog.ReadScopeAsync(owner);
            Check(Math.Abs(saved.Selection.TabScrollViewerHorizontalOffset - 120) < 1 &&
                saved.Selection.SelectedEditorId == ids[3], "Checkpoint lost the restored tab-strip position or selection.");

            var scroll = Descendants(sets).OfType<ScrollViewer>().First(item => item.Name == "ScrollViewer");
            var reentered = false;
            void Resized(object sender, SizeChangedEventArgs args)
            {
                reentered = true;
                sets.SizeChanged -= Resized;
                sets.ScrollTo(40);
            }
            sets.SizeChanged += Resized;
            sets.Width = 550;
            sets.ScrollTo(200);
            await UntilAsync(() => reentered && Math.Abs(scroll.HorizontalOffset - 40) < 1,
                "A newer reentrant scroll request was overwritten.");

            Window.Current.Content = new Grid();
            await UntilAsync(() => !sets.IsLoaded, "Startup fixture did not detach.");
            sets.ScrollTo(250);
            core.SwitchTo(core.GetTabs()[0]);
            Window.Current.Content = frame;
            await UntilAsync(() => sets.IsLoaded && scroll.HorizontalOffset < 1,
                "Explicit tab reveal did not replace the detached saved offset.");
            Check(core.GetSelectedTab()?.Id == ids[0], "Reattachment changed the explicitly selected tab.");
            await VerifySelectionDuringPendingScrollAsync(page, sets, scroll);
        }
        finally
        {
            if (observed != null) observed.Painted -= OnPainted;
            await page.TestDisposeWindowAsync();
        }
    }

    private static async Task VerifySelectionDuringPendingScrollAsync(NotepadsMainPage page, SetsView sets, ScrollViewer scroll)
    {
        var core = page.TestCore;
        var tabs = core.GetTabs();
        var last = tabs.Length - 1;
        // The scroll buttons sit outside this presenter. Its origin, unlike the
        // outer ScrollViewer's origin, is the left edge of the visible headers.
        var viewport = Descendants(scroll).OfType<FrameworkElement>()
            .Single(element => element.Name == "ScrollContentPresenter");
        // Invoke the same core commands as the MainPage keyboard bindings. This
        // tests their layout ordering, without depending on OS keyboard injection.
        var cases = new (string Name, int Start, int Target, Action Select)[]
        {
            ("Ctrl+1", last, 0, () => core.SwitchTo(0)),
            ("Ctrl+8", 0, last, () => core.SwitchTo(last)),
            ("Ctrl+Tab wrap", last, 0, () => core.SwitchTo(true)),
            ("Ctrl+Shift+Tab wrap", 0, last, () => core.SwitchTo(false)),
            ("header selection", 0, 1, () =>
            {
                // Select the real container rather than going through Core's
                // SwitchTo overloads. This exercises the SelectionChanged path
                // used by a header click; it does not inject an OS pointer event.
                page.TestContainer(tabs[1]).IsSelected = true;
            })
        };
        foreach (var pendingReveal in new[] { false, true })
        {
            foreach (var scenario in cases)
            {
                var name = scenario.Name + (pendingReveal ? " / pending reveal" : " / pending offset");
                core.SwitchTo(tabs[scenario.Start]);
                await UntilAsync(() => core.GetSelectedReadyEditor() != null, "The starting tab is ready: " + name);
                await VisibleAcrossFramesAsync(page.TestContainer(tabs[scenario.Start]), viewport, scroll, name + " setup");
                if (scenario.Name == "header selection")
                    Check(IsFullyVisible(page.TestContainer(tabs[scenario.Target]), viewport),
                        "The header selection target is visible before the competing request.");

                // Queue the conflicting request and select in one UI turn, so the
                // old intent definitely has not reached the next layout pass.
                var obsoleteIndex = scenario.Target == last ? 0 : last;
                if (pendingReveal) sets.ScrollSetIntoView(page.TestContainer(tabs[obsoleteIndex]));
                else sets.ScrollTo(obsoleteIndex == 0 ? 0 : scroll.ScrollableWidth);
                scenario.Select();
                Check(core.GetSelectedTab() == tabs[scenario.Target], "The command selected its requested tab: " + name);
                await UntilAsync(() => core.GetSelectedReadyEditor() != null, "The selected tab is ready: " + name);
                await VisibleAcrossFramesAsync(page.TestContainer(tabs[scenario.Target]), viewport, scroll, name);
                Check(core.GetSelectedTab() == tabs[scenario.Target], "Layout kept the latest selection: " + name);
            }
        }
    }

    private static bool IsFullyVisible(SetsViewItem item, FrameworkElement viewport)
    {
        var left = item.TransformToVisual(viewport).TransformPoint(new Point()).X;
        return item.ActualWidth > 0 && left >= -1 && left + item.ActualWidth <= viewport.ActualWidth + 1;
    }

    private static async Task VisibleAcrossFramesAsync(SetsViewItem item, FrameworkElement viewport, ScrollViewer scroll, string scenario)
    {
        var rendered = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        var visibleFrames = 0;
        void Rendering(object sender, object args)
        {
            visibleFrames = IsFullyVisible(item, viewport) ? visibleFrames + 1 : 0;
            if (visibleFrames == 3) rendered.TrySetResult(true);
        }
        CompositionTarget.Rendering += Rendering;
        try { await rendered.Task.WaitAsync(TimeSpan.FromSeconds(5)); }
        catch (TimeoutException)
        {
            throw new InvalidOperationException($"Selected header was not kept visible: {scenario}; " +
                $"left={item.TransformToVisual(viewport).TransformPoint(new Point()).X}, width={item.ActualWidth}, " +
                $"offset={scroll.HorizontalOffset}, viewport={scroll.ViewportWidth}, extent={scroll.ExtentWidth}.");
        }
        finally { CompositionTarget.Rendering -= Rendering; }
    }
}
