// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.ApplicationModel.DataTransfer;
using static NotepadsWorkspaceTests.DocumentFeatureAssertions;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class ClipboardContractTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var smartCopy = ApplicationPreferences.IsSmartCopyEnabled;
        try
        {
            ApplicationPreferences.IsSmartCopyEnabled = false;
            TestClipboard.Reset();
            var editor = await OpenAsync(host, "clipboard.txt", [0x61, 0x6c, 0x70, 0x68, 0x61, 0x0d, 0x0a, 0x62, 0x65, 0x74, 0x61]);
            editor.TryChangeLineEnding(LineEnding.Crlf);
            editor.TestNativeCore.SetTextSelectionPosition(2, 2);
            editor.CopyTextToWindowsClipboard(null);
            Check(await TestClipboard.GetContent().GetTextAsync() == "alpha\r\n", "Copy with no selection must publish the current line with the document EOL.");
            Selection(editor, 2, 2, "Copy current line must preserve the caret");
            Check(editor.GetText() == "alpha\rbeta" && !editor.IsModified, "Copy must preserve text and the native save point.");
            Check(TestClipboard.FlushCount == 1, "A successful managed copy must request clipboard persistence exactly once at the declared boundary.");
            Check(TestClipboard.GetContent().RequestedOperation == DataPackageOperation.Copy &&
                TestClipboard.LastOptions.IsAllowedInHistory && TestClipboard.LastOptions.IsRoamable,
                "Managed copy must declare its payload operation and persistence options at the isolated clipboard boundary.");
            log.AppendLine("PASS: clipboard/copy-line preserves caret and emits document line endings through DataPackage.");

            editor.TestNativeCore.SetTextSelectionPosition(6, 10);
            editor.CutSelectedTextToWindowsClipboard(null);
            Check(await TestClipboard.GetContent().GetTextAsync() == "beta" && editor.GetText() == "alpha\r", "Cut must publish exactly the selected text and delete that selection.");
            editor.TestNativeCore.Undo();
            Check(editor.GetText() == "alpha\rbeta" && !editor.IsModified, "One undo must restore cut text and the save point.");
            log.AppendLine("PASS: clipboard/cut selected text uses DataPackage and restores in one undo.");

            editor.TestNativeCore.SetTextSelectionPosition(2, 2);
            editor.CutSelectedTextToWindowsClipboard(null);
            Check(await TestClipboard.GetContent().GetTextAsync() == "alpha\r\n" && editor.GetText() == "beta" && editor.IsModified,
                "Successful Cut without a selection must publish and remove the whole current line including its line ending.");
            Selection(editor, 0, 0, "Cutting the current line must collapse the caret at the remaining line start");
            editor.TestNativeCore.Undo();
            Check(editor.GetText() == "alpha\rbeta" && !editor.IsModified,
                "One undo must restore the whole-line cut and its original save point.");
            log.AppendLine("PASS: clipboard/cut-line publishes and removes the current line, with one-step undo.");

            var package = new DataPackage();
            package.SetText("Ω\r\n🙂\nlast");
            TestClipboard.SetContentWithOptions(package, new ClipboardContentOptions());
            editor.TestNativeCore.SetTextSelectionPosition(0, 5);
            await editor.TestNativeCore.PastePlainTextFromWindowsClipboardAsync(null);
            Check(editor.GetText() == "Ω\r🙂\rlast\rbeta", "Managed paste must replace selection and normalize mixed EOL without changing Unicode.");
            editor.TestNativeCore.Undo();
            Check(editor.GetText() == "alpha\rbeta", "Managed paste must be one undo unit.");
            log.AppendLine("PASS: clipboard/managed-paste normalizes mixed EOL and replaces selection atomically.");

            var smart = await OpenAsync(host, "smart-copy.txt", Encoding.UTF8.GetBytes("\r\n  Ω value  \r\n  "));
            ApplicationPreferences.IsSmartCopyEnabled = true;
            smart.TestNativeCore.SetTextSelectionPosition(0, "\r  Ω value  \r  ".Length);
            smart.CopyTextToWindowsClipboard(null);
            Check(await TestClipboard.GetContent().GetTextAsync() == "  Ω value", "Smart copy removes leading blank lines and trailing whitespace but preserves meaningful indentation.");
            Selection(smart, 1, 10, "Smart copy must leave exactly the trimmed document range selected");
            Check(!smart.IsModified, "Smart copy must not modify the document.");
            log.AppendLine("PASS: clipboard/smart-copy trims blank lines and trailing whitespace while retaining indentation.");

            VerifyRejectedCut(editor, log);
        }
        finally
        {
            TestClipboard.Reset();
            ApplicationPreferences.IsSmartCopyEnabled = smartCopy;
        }
    }

    internal static async Task RunRejectedCutAsync(StringBuilder log)
    {
        await using var host = await FeatureTestHost.CreateAsync();
        var smartCopy = ApplicationPreferences.IsSmartCopyEnabled;
        try
        {
            var editor = await OpenAsync(host, "rejected-cut.txt", [0x61, 0x6c, 0x70, 0x68, 0x61, 0x0d, 0x0a, 0x62, 0x65, 0x74, 0x61]);
            VerifyRejectedCut(editor, log);
        }
        finally
        {
            TestClipboard.Reset();
            ApplicationPreferences.IsSmartCopyEnabled = smartCopy;
        }
    }

    private static void VerifyRejectedCut(TextEditor editor, StringBuilder log)
    {
        ApplicationPreferences.IsSmartCopyEnabled = false;
        TestClipboard.Reset();
        TestClipboard.AcceptWrites = false;
        editor.TestNativeCore.SetTextSelectionPosition(2, 2);
        editor.CutSelectedTextToWindowsClipboard(null);
        Check(editor.GetText() == "alpha\rbeta" && !editor.IsModified, "Rejected clipboard write must not cut document text.");
        Selection(editor, 2, 2, "Rejected clipboard write must preserve the original caret instead of selecting the current line");
        Check(TestClipboard.FlushCount == 0, "Rejected clipboard content must never be flushed.");
        foreach (var duringFlush in new[] { false, true })
        {
            TestClipboard.Reset();
            if (duringFlush) TestClipboard.FlushFailure = new InvalidOperationException("Unavailable clipboard persistence.");
            else TestClipboard.WriteFailure = new InvalidOperationException("Clipboard is unavailable.");
            editor.CutSelectedTextToWindowsClipboard(null);
            Check(editor.GetText() == "alpha\rbeta" && !editor.IsModified, "A clipboard exception must preserve document text.");
            Selection(editor, 2, 2, "A clipboard exception must restore the temporary line selection");
        }
        TestClipboard.Reset();
        TestClipboard.AcceptWrites = false;
        editor.TestNativeCore.SetTextSelectionPosition(1, 4);
        editor.CutSelectedTextToWindowsClipboard(null);
        Selection(editor, 1, 4, "Rejected Cut must preserve an existing nonempty selection");
        log.AppendLine("PASS: clipboard/rejected-cut preserves text, save point and caret.");
    }
}
