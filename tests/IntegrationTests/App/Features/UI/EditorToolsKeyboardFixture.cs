// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using WinUIEditor;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

// Supplies a real workspace and observations only. The desktop client opens
// Find All and dismisses it using physical keys through the XAML event route.
internal sealed class EditorToolsKeyboardFixture : IAsyncDisposable
{
    private readonly FeatureTestHost _host;
    private readonly UIElement _previousContent;
    private readonly TextEditor _editor;

    private EditorToolsKeyboardFixture(FeatureTestHost host, UIElement previousContent)
    {
        _host = host;
        _previousContent = previousContent;
        _editor = (TextEditor)host.Core.GetSelectedReadyEditor();
    }

    internal static async Task<EditorToolsKeyboardFixture> CreateAsync()
    {
        var previous = Window.Current.Content;
        FeatureTestHost host = null;
        try
        {
            host = await FeatureTestHost.CreateAsync();
            var fixture = new EditorToolsKeyboardFixture(host, previous);
            var editor = fixture._editor;
            editor.TypeText("cat one\rcat two\rcat three");
            editor.TestNativeCore.TestFeatureNative.SetSel(0, 0);
            var view = (EditorBaseControl)((Grid)editor.TestNativeCore.Content).Children[0];
            AutomationProperties.SetAutomationId(view, "EditorToolsEditor");
            editor.ShowFindAndReplaceControl(false);
            var query = Named<FindAndReplaceControl>(editor, "FindAndReplaceControl").TestFindInput;
            query.Text = "cat";
            AutomationProperties.SetAutomationId(query, "EditorToolsQuery");
            await UntilAsync(() => query.IsLoaded && query.ActualWidth > 0 && editor.TestFindVisible,
                "The query is presented before desktop keyboard input.");
            return fixture;
        }
        catch
        {
            try { if (host != null) await host.DisposeAsync(); }
            finally { Window.Current.Content = previous; }
            throw;
        }
    }

    internal string Snapshot()
    {
        var list = _editor.TestFindAllList;
        var first = list?.ContainerFromIndex(0) as ListViewItem;
        var selected = list?.ContainerFromIndex(list.SelectedIndex) as ListViewItem;
        if (first != null) AutomationProperties.SetAutomationId(first, "EditorToolsResult");
        return $"toolsMode={_editor.Mode}\ntoolsResultsVisible={_editor.TestFindAllVisible}\ntoolsResultCount={list?.Items.Count ?? 0}\n" +
            $"toolsResultReady={first?.IsLoaded == true}\n" +
            $"toolsSelectedResult={list?.SelectedIndex ?? -1}\ntoolsResultFocus={selected?.FocusState}\n" +
            $"toolsQueryVisible={_editor.TestFindVisible}\ntoolsText64=" +
            Convert.ToBase64String(Encoding.UTF8.GetBytes(_editor.GetText())) + "\n";
    }

    public async ValueTask DisposeAsync()
    {
        try { await _host.DisposeAsync(); }
        finally { Window.Current.Content = _previousContent; }
    }
}
