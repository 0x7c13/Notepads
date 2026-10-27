// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Recovery;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using static NotepadsIntegrationTests.FeatureUI;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsIntegrationTests;

internal static class IndentationFeatureTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var tab = host.Core.GetSelectedTab();
        var editor = (TextEditor)tab.ReadyEditor;
        host.Page.ShowHideStatusBar(true);
        var button = (Button)host.Page.FindName("IndentationIndicatorButton");
        var label = (TextBlock)host.Page.FindName("IndentationIndicator");
        var flyout = (MenuFlyout)FlyoutBase.GetAttachedFlyout(button);
        Invoke(button);
        await UntilAsync(() => flyout.Items.Count == 8, "The indentation menu opens from the actual status bar.");
        Invoke(flyout.Items.OfType<MenuFlyoutItem>().Single(item => item.Tag is int value && value == 8));
        Check(editor.IndentationOverride == 8 && editor.EffectiveIndentation == 8 && !editor.IsModified &&
            label.Text.Contains('8') && AutomationProperties.GetName(label).Contains('8'),
            "A document override updates the status and accessibility without modifying text.");
        flyout.Hide();
        editor.TypeText("\t中文\r \t x\r");
        var original = editor.GetText();
        await editor.ConvertIndentationAsync(false);
        Check(editor.GetText() == "    中文\r     x\r" && editor.IndentationOverride == 8,
            "Converting prefixes preserves visual columns and the explicitly selected space unit.");
        using (var captured = editor.CaptureRecoveryState())
        {
            await captured.Checkpoint.FlushAsync();
            var restored = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "Converted recovery");
            try
            {
                await restored.RestoreV2Async(captured.SavedSnapshot, captured.RecoveryBaseline, captured.DocumentJournal,
                    captured.Checkpoint, null, captured.MetaData, captured.TextDirty);
                Check(restored.GetText() == editor.GetText() && restored.IndentationOverride == 8 && restored.IsModified,
                    "The conversion is recoverable through the real durable journal together with its document policy.");
            }
            finally { await host.Core.DiscardUnattachedEditorAsync(restored); }
        }
        editor.TestNativeCore.Undo();
        Check(editor.GetText() == original, "One undo reverses the complete conversion through the real editor.");

        var second = await NewTabAsync(host.Page);
        var other = (TextEditor)second.ReadyEditor;
        other.SetIndentationOverride(2);
        editor.SetIndentationOverride(-1);
        Check(label.Text.Contains('2') && other.IndentationOverride == 2,
            "Changing a background document does not overwrite the selected status or another document's policy.");
        host.Core.SwitchTo(tab);
        Check(!label.Text.Contains('2') && editor.GetTextEditorStateMetaData().IndentationOverride == -1,
            "Switching tabs restores the document's own indentation indicator and metadata.");
        host.Page.TestController.StopSessionBackup();
        host.Page.TestController.IsBackupEnabled = true;
        Check(await host.Page.TestController.SaveSessionAsync(), "Indentation overrides are saved in a real session checkpoint.");
        var scope = await SessionRecoveryCatalog.ReadScopeAsync(host.Core.DocumentOwnerId);
        Check(scope.Selection.Documents.Single(row => row.Id == editor.Id).Descriptor.StateMetaData.IndentationOverride == -1 &&
            scope.Selection.Documents.Single(row => row.Id == other.Id).Descriptor.StateMetaData.IndentationOverride == 2,
            "The durable session retains distinct indentation policies for both documents.");
        editor.SetLanguageOverride("yaml");
        Check(editor.EffectiveIndentation == 2, "Changing a tabs document to YAML enforces spaces.");
        var before = editor.GetText();
        await editor.ConvertIndentationAsync(true);
        Check(editor.GetText() == before, "The public conversion command also refuses YAML tabs.");
        editor.SetLanguageOverride("plaintext");
        editor.TestNativeCore.SelectAll();
        editor.TypeText(string.Empty);
        editor.SetIndentationOverride(0);
        editor.TypeText("a\r  b\r    c\r");
        await UntilAsync(() => label.Text.Contains('2'), "Pasted indentation refreshes the selected status after input settles.");
        var closing = (TextEditor)host.Core.CreateTextEditor(Guid.NewGuid(), null, "Cancelled conversion");
        try
        {
            await closing.InitializeNewAsync();
            closing.TypeText("\tclosing\r");
            var conversion = closing.ConvertIndentationAsync(false);
            Check(!closing.CanChangeIndentation, "An admitted conversion blocks concurrent document policy changes.");
            await host.Core.DiscardUnattachedEditorAsync(closing);
            try { await conversion; throw new InvalidOperationException("Closing did not cancel the conversion."); }
            catch (OperationCanceledException) { }
        }
        finally { await host.Core.DiscardUnattachedEditorAsync(closing); }
        log.AppendLine("PASS: document indentation status, accessible menu, per-tab policy, conversion undo/recovery/cancellation, session checkpoint and YAML guards.");
    }
}
