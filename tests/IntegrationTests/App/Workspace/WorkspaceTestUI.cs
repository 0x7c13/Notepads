// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace NotepadsWorkspaceTests;

internal static class WorkspaceTestUI
{
    internal static void Check(bool condition, string message)
    {
        File.AppendAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "progress.txt"),
            (condition ? "OK: " : "FAIL: ") + message + Environment.NewLine);
        if (!condition) throw new InvalidOperationException(message);
    }

    internal static async Task UntilAsync(Func<bool> condition, string message, int milliseconds = 15000)
    {
        File.AppendAllText(Path.Combine(ApplicationData.Current.LocalFolder.Path, "progress.txt"), "WAIT: " + message + Environment.NewLine);
        var deadline = DateTime.UtcNow.AddMilliseconds(milliseconds);
        while (!condition())
        {
            if (DateTime.UtcNow > deadline) throw new TimeoutException(message);
            await Task.Delay(25);
        }
    }

    // Invokes the actual template button: ContentDialog handlers and DialogManager still run.
    internal static async Task ChooseDialogAsync(Task command, string buttonName, string expectedContent = null,
        Action beforeInvoke = null, Func<Task> whileDialogOpen = null)
    {
        ContentDialog dialog = null;
        await UntilAsync(() =>
        {
            dialog = VisualTreeHelper.GetOpenPopups(Window.Current).SelectMany(popup => Descendants(popup.Child))
                .OfType<ContentDialog>().FirstOrDefault();
            return dialog != null || command.IsCompleted;
        }, "The command never presented the expected dialog.");
        Check(dialog != null, "The command completed without the expected dialog.");
        if (expectedContent != null) Check(dialog.Content?.ToString()?.Contains(expectedContent, StringComparison.Ordinal) == true,
            "The dialog did not contain the expected content: " + expectedContent);
        Button button = null;
        await UntilAsync(() =>
        {
            button = Descendants(dialog).OfType<Button>().FirstOrDefault(candidate => candidate.Name == buttonName);
            return button?.IsEnabled == true;
        }, "The dialog button was not enabled: " + buttonName);
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button) ?? new ButtonAutomationPeer(button);
        Check(peer.GetPattern(PatternInterface.Invoke) is IInvokeProvider, "Dialog button has no invocation provider.");
        if (whileDialogOpen != null) await whileDialogOpen();
        beforeInvoke?.Invoke();
        ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        await command.WaitAsync(TimeSpan.FromSeconds(20));
    }

    internal static IEnumerable<DependencyObject> Descendants(DependencyObject root)
    {
        if (root == null) yield break;
        yield return root;
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(root); index++)
            foreach (var child in Descendants(VisualTreeHelper.GetChild(root, index))) yield return child;
    }
}
