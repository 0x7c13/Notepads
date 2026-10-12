// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Replace outbound OS entry points only. Factory, parsing, command, document,
// print-content and error-handling implementations remain production code.
global using FileOpenPicker = NotepadsIntegrationTests.TestFileOpenPicker;
global using Launcher = NotepadsIntegrationTests.TestLauncher;
global using PrintManager = NotepadsIntegrationTests.TestPrintManager;

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Windows.Foundation;
using Windows.Graphics.Printing;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.System;

namespace NotepadsIntegrationTests;

public sealed class TestFileOpenPicker
{
    public PickerLocationId SuggestedStartLocation { get; set; }
    public IList<string> FileTypeFilter { get; } = new List<string>();
    internal static Queue<IReadOnlyList<StorageFile>> Results { get; } = new();
    internal static TestFileOpenPicker LastCreated { get; private set; }
    internal static Exception Failure { get; set; }
    public TestFileOpenPicker() => LastCreated = this;
    public Task<IReadOnlyList<StorageFile>> PickMultipleFilesAsync()
    {
        if (Failure != null) return Task.FromException<IReadOnlyList<StorageFile>>(Failure);
        return Results.Count == 0
            ? throw new InvalidOperationException("Open picker invoked without an explicit test result.")
            : Task.FromResult(Results.Dequeue());
    }
    internal static void Reset() { Results.Clear(); LastCreated = null; Failure = null; }
}

internal static class TestLauncher
{
    internal sealed record Request(Uri Uri, LauncherOptions Options, string FolderPath);
    internal static List<Request> Requests { get; } = new();
    internal static bool Result { get; set; } = true;
    internal static Exception Failure { get; set; }
    public static Task<bool> LaunchUriAsync(Uri uri) => LaunchUriAsync(uri, null);
    public static Task<bool> LaunchUriAsync(Uri uri, LauncherOptions options)
    {
        Requests.Add(new Request(uri, options, null));
        return Complete();
    }
    public static Task<bool> LaunchFolderPathAsync(string path)
    {
        Requests.Add(new Request(null, null, path));
        return Complete();
    }
    public static Task<bool> LaunchFolderAsync(StorageFolder folder) => LaunchFolderPathAsync(folder.Path);
    private static Task<bool> Complete() => Failure == null ? Task.FromResult(Result) : Task.FromException<bool>(Failure);
    internal static void Reset() { Requests.Clear(); Result = true; Failure = null; }
}

internal sealed class TestPrintManager
{
    private static readonly TestPrintManager Current = new();
    private TypedEventHandler<TestPrintManager, PrintTaskRequestedEventArgs> _requested;
    internal static bool Supported { get; set; } = true;
    internal static int ShowCount { get; private set; }
    internal static Exception ShowFailure { get; set; }
    internal static int SubscriptionCount => Current._requested?.GetInvocationList().Length ?? 0;
    public event TypedEventHandler<TestPrintManager, PrintTaskRequestedEventArgs> PrintTaskRequested
    {
        add => _requested += value;
        remove => _requested -= value;
    }
    public static bool IsSupported() => Supported;
    public static TestPrintManager GetForCurrentView() => Current;
    public static Task<bool> ShowPrintUIAsync()
    {
        ShowCount++;
        return ShowFailure == null ? Task.FromResult(true) : Task.FromException<bool>(ShowFailure);
    }
    // Do not reset subscriptions: that would conceal production lifetime leaks.
    internal static void ResetRequests() { ShowCount = 0; ShowFailure = null; Supported = true; }
}
