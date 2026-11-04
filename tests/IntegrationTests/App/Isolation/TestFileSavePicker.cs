// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

global using FileSavePicker = NotepadsIntegrationTests.TestFileSavePicker;

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Windows.Storage;
using Windows.Storage.Pickers;

namespace NotepadsIntegrationTests;

// Only the OS chooser is replaced; the real picker factory, Save As command,
// writer, document identity and cancellation paths execute unchanged.
public sealed class TestFileSavePicker
{
    public PickerLocationId SuggestedStartLocation { get; set; }
    public string SuggestedFileName { get; set; }
    public IDictionary<string, IList<string>> FileTypeChoices { get; } = new Dictionary<string, IList<string>>();
    internal static Queue<StorageFile> Results { get; } = new();
    internal static TestFileSavePicker LastCreated { get; private set; }
    public TestFileSavePicker() => LastCreated = this;
    public Task<StorageFile> PickSaveFileAsync() => Results.Count == 0
        ? throw new InvalidOperationException("Save picker was invoked without an explicit test result.")
        : Task.FromResult(Results.Dequeue());
    internal static void Reset() { Results.Clear(); LastCreated = null; }
}
