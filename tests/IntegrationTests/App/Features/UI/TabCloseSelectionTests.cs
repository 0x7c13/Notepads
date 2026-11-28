// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Workspace;
using Windows.UI.Xaml;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class TabCloseSelectionTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        foreach (var direction in new[] { FlowDirection.LeftToRight, FlowDirection.RightToLeft })
        {
            await using var host = await FeatureTestHost.CreateAsync();
            host.Page.FlowDirection = direction;
            var first = host.Core.GetSelectedTab();
            var second = await NewTabAsync(host.Page);
            var third = await NewTabAsync(host.Page);
            await CloseAsync(third, second, "Closing the selected last tab chooses its previous neighbor.");

            third = await NewTabAsync(host.Page);
            host.Core.SwitchTo(second);
            await CloseAsync(second, third, "Closing the selected middle tab chooses its next neighbor.");
            host.Core.SwitchTo(first);
            await CloseAsync(first, third, "Closing the selected first tab chooses its next neighbor.");

            var fourth = await NewTabAsync(host.Page);
            var fifth = await NewTabAsync(host.Page);
            host.Core.SwitchTo(fourth);
            await CloseAsync(third, fourth, "Closing a background tab before the selection preserves the selected document.");
            await CloseAsync(fifth, fourth, "Closing a background tab after the selection preserves the selected document.");

            // Reordering changes adjacency; identity and creation order must not
            // become a substitute for the current order of the tab strip.
            var sixth = await NewTabAsync(host.Page);
            var seventh = await NewTabAsync(host.Page);
            var sets = Named<Notepads.Controls.SetsView>(host.Page, "Sets");
            Check(sets.MoveSet(host.Page.TestContainer(fourth), 1), "The close fixture reorders its tabs.");
            Check(host.Core.GetTabs().SequenceEqual(new[] { sixth, fourth, seventh }), "The reordered roster has the intended neighbors.");
            await CloseAsync(seventh, fourth, "Closing after reorder follows displayed order, not creation order.");

            // A transfer removes the source directly without the page's close
            // guard. Both removal paths must choose the same neighbor.
            host.Core.DeleteTab(fourth);
            Check(ReferenceEquals(host.Core.GetSelectedTab(), sixth), "Direct source removal selects the remaining neighbor.");

            var dirty = await NewTabAsync(host.Page);
            Editor(dirty).TypeText("Keep this draft after cancelling close.");
            await ChooseDialogAsync(host.Page.TestCloseTabAsync(dirty), "CloseButton");
            Check(host.Core.GetTabs().SequenceEqual(new[] { sixth, dirty }) &&
                ReferenceEquals(host.Core.GetSelectedTab(), dirty) &&
                Editor(dirty).GetText() == "Keep this draft after cancelling close.",
                "Cancelling the real save reminder preserves the draft, selection and roster.");
            await ChooseDialogAsync(host.Page.TestCloseTabAsync(dirty), "SecondaryButton");
            Check(host.Core.GetTabs().Single() == sixth && ReferenceEquals(host.Core.GetSelectedTab(), sixth),
                "Confirmed discard chooses the same adjacent tab as a clean close.");

            async Task CloseAsync(WorkspaceTab closing, WorkspaceTab expected, string message)
            {
                var before = host.Core.GetTabs();
                var replacingSelection = ReferenceEquals(host.Core.GetSelectedTab(), closing);
                var selected = new List<WorkspaceTab>();
                void OnSelected(object sender, WorkspaceTab tab) { if (tab != null) selected.Add(tab); }
                host.Core.TabSelected += OnSelected;
                try { await host.Page.TestCloseTabAsync(closing); }
                finally { host.Core.TabSelected -= OnSelected; }
                Check(!host.Core.ContainsTab(closing) && host.Page.IsEnabled &&
                    host.Core.GetTabs().SequenceEqual(before.Where(tab => tab != closing)),
                    "The real close flow removes only its document and restores interaction.");
                Check(ReferenceEquals(host.Core.GetSelectedTab(), expected),
                    message + " Direction=" + direction + "; selected index=" +
                    Array.IndexOf(host.Core.GetTabs(), host.Core.GetSelectedTab()));
                Check((!replacingSelection || selected.Count > 0) && selected.All(tab => ReferenceEquals(tab, expected)),
                    "Closing never transiently selects an unrelated document.");
                await UntilAsync(() => host.Page.TestContentHost(expected).IsLoaded,
                    "The selected neighbor is presented in the actual workspace.");
            }
        }
        log.AppendLine("PASS: Tab close selection — selected first/middle/last, background tabs, reorder and direct removal in LTR/RTL.");
        log.AppendLine("PASS: Tab close selection — cancelled save reminders preserve drafts; confirmed discard selects the neighbor.");
    }
}
