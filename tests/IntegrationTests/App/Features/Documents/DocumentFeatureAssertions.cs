// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using NotepadsIntegrationTests;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using static NotepadsWorkspaceTests.WorkspaceTestUI;

namespace NotepadsWorkspaceTests;

internal static class DocumentFeatureAssertions
{
    internal static async Task<TextEditor> OpenAsync(FeatureTestHost host, string name, byte[] bytes)
    {
        var file = await host.Folder.CreateFileAsync(name, CreationCollisionOption.FailIfExists);
        await FileIO.WriteBytesAsync(file, bytes);
        Check(await host.Page.OpenFileAsync(file), "Opening a new document must create its actual workspace editor: " + name);
        var editor = (TextEditor)host.Core.GetSelectedReadyEditor();
        await editor.TestStopFileChecksAsync();
        await UntilAsync(() => editor.TestNativeCore.IsLoaded, "The native document control was not loaded: " + name);
        return editor;
    }

    internal static void Selection(TextEditor editor, int start, int end, string message)
    {
        editor.TestNativeCore.GetTextSelectionPosition(out var actualStart, out var actualEnd);
        Check(actualStart == start && actualEnd == end,
            message + $" (expected [{start},{end}], actual [{actualStart},{actualEnd}]).");
    }

    internal static async Task BytesAsync(StorageFile file, byte[] expected, string message)
    {
        using var stream = await file.OpenStreamForReadAsync();
        using var copy = new System.IO.MemoryStream();
        await stream.CopyToAsync(copy);
        var actual = copy.ToArray();
        Check(actual.SequenceEqual(expected), message + " (expected " + Convert.ToHexString(expected) +
            ", actual " + Convert.ToHexString(actual) + ").");
    }

    internal static void Invoke(Button button)
    {
        Check(button.IsEnabled && button.Visibility == Visibility.Visible, "The requested action button must be available: " + button.Name);
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button) ?? new ButtonAutomationPeer(button);
        Check(peer.GetPattern(PatternInterface.Invoke) is IInvokeProvider, "The action button must have an Invoke provider: " + button.Name);
        ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
    }

    internal static async Task ThrowsAsync<T>(Func<Task> action, string message) where T : Exception
    {
        try { await action(); }
        catch (T) { return; }
        throw new InvalidOperationException(message);
    }
}
