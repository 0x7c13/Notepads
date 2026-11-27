// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.Dialog;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Automation.Provider;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;
using WinRT;
using WinRT.Interop;

namespace NotepadsWorkspaceTests;

internal static partial class WorkspaceTestUI
{
    // ApplicationView.Title reads empty on this Windows host even when the
    // visible shell caption is correct. Read the actual window, not setter intent.
    internal static unsafe string ActualWindowTitle()
    {
        using var interop = ((IWinRTObject)Window.Current.CoreWindow).NativeObject.As<IUnknownVftbl>(
            new Guid("45D64A29-A63E-4CB6-B498-5781D298CB4F")); // ICoreWindowInterop, CoreWindow.h
        var self = interop.ThisPtr;
        nint core = 0;
        var getHandle = (delegate* unmanaged[Stdcall]<nint, nint*, int>)(*(nint**)self)[3];
        Marshal.ThrowExceptionForHR(getHandle(self, &core));
        var root = NativeMethods.GetAncestor(core, 2 /* GA_ROOT */);
        if (root == 0) throw new InvalidOperationException("The owned CoreWindow has no root window.");
        Span<char> text = stackalloc char[2048];
        fixed (char* pointer = text)
            return new string(text[..NativeMethods.GetWindowTextW(root, pointer, text.Length)]);
    }

    private static partial class NativeMethods
    {
        [LibraryImport("user32.dll")]
        internal static partial nint GetAncestor(nint window, uint flags);

        [LibraryImport("user32.dll")]
        internal static unsafe partial int GetWindowTextW(nint window, char* text, int capacity);
    }

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
            dialog = ActiveVisibleDialog();
            return dialog != null || command.IsCompleted;
        }, "The command never presented the expected dialog.");
        Check(dialog != null, "The command completed without the expected dialog.");
        if (expectedContent != null) Check(dialog.Content?.ToString()?.Contains(expectedContent, StringComparison.Ordinal) == true,
            "The dialog did not contain the expected content: " + expectedContent);
        Button button = null;
        await UntilAsync(() =>
        {
            if (!ReferenceEquals(dialog, ActiveVisibleDialog())) return false;
            button = Descendants(dialog).OfType<Button>().FirstOrDefault(candidate => candidate.Name == buttonName);
            return button?.IsEnabled == true;
        }, "The dialog button was not enabled: " + buttonName);
        var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button) ?? new ButtonAutomationPeer(button);
        Check(peer.GetPattern(PatternInterface.Invoke) is IInvokeProvider, "Dialog button has no invocation provider.");
        if (whileDialogOpen != null) await whileDialogOpen();
        beforeInvoke?.Invoke();
        Check(ReferenceEquals(dialog, ActiveVisibleDialog()), "The expected dialog is no longer active and visible.");
        ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        await command.WaitAsync(TimeSpan.FromSeconds(20));
    }

    internal static NotepadsDialog ActiveVisibleDialog() => VisualTreeHelper.GetOpenPopups(Window.Current)
        .SelectMany(popup => Descendants(popup.Child)).OfType<NotepadsDialog>()
        .FirstOrDefault(dialog => ReferenceEquals(dialog, DialogManager.ActiveDialog));

    internal static IEnumerable<DependencyObject> Descendants(DependencyObject root)
    {
        if (root == null) yield break;
        yield return root;
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(root); index++)
            foreach (var child in Descendants(VisualTreeHelper.GetChild(root, index))) yield return child;
    }
}
