// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
// This alias applies to source-linked product code as well as the test cases.
global using ApplicationData = NotepadsTestIsolation.TestApplicationData;

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Windows.ApplicationModel;
using Windows.ApplicationModel.Activation;
using Windows.Storage;
using Windows.Storage.AccessCache;

namespace NotepadsTestIsolation;

internal sealed class TestApplicationData
{
    private static TestApplicationData _current;
    private static Dictionary<string, string> _originalFileGrants;

    private TestApplicationData(StorageFolder local, StorageFolder temporary)
    {
        LocalFolder = local;
        TemporaryFolder = temporary;
    }

    public static TestApplicationData Current => _current ??
        throw new InvalidOperationException("Test storage must be initialized before constructing the application.");
    public static Guid RunId { get; private set; }
    public static string Arguments { get; private set; }
    public StorageFolder LocalFolder { get; }
    public StorageFolder TemporaryFolder { get; }

    // Called on the entry thread before Application.Start and before any diagnostics.
    // A fresh process must explicitly reuse the same ID to reopen a prior fixture.
    public static void InitializeFromActivation()
    {
        if (_current != null) throw new InvalidOperationException("Test storage is already initialized.");
        if (AppInstance.GetActivatedEventArgs() is not LaunchActivatedEventArgs activation)
            throw new InvalidOperationException("Only explicit test launch activation is supported.");
        const string prefix = "--test-run ";
        var command = activation.Arguments ?? "";
        if (!command.StartsWith(prefix, StringComparison.Ordinal))
            throw new ArgumentException("An explicit --test-run <GUID> is required; package-root storage is prohibited.");
        var remainder = command.Substring(prefix.Length);
        var separator = remainder.IndexOf(' ');
        var id = separator < 0 ? remainder : remainder.Substring(0, separator);
        if (!Guid.TryParseExact(id, "N", out var runId) || runId == Guid.Empty)
            throw new ArgumentException("The test run ID must be a nonempty GUID in N format.");
        RunId = runId;
        Arguments = separator < 0 ? "" : remainder.Substring(separator + 1);
        _current = Task.Run(async () =>
        {
            // This is the only permitted package-root access in either test assembly.
            var root = Windows.Storage.ApplicationData.Current.LocalFolder;
            var workspace = await root.CreateFolderAsync("WorkspaceTests", CreationCollisionOption.OpenIfExists);
            var local = await workspace.CreateFolderAsync(runId.ToString("N"), CreationCollisionOption.OpenIfExists);
            var temporary = await local.CreateFolderAsync("Temporary", CreationCollisionOption.OpenIfExists);
            var expected = Path.Combine(root.Path, "WorkspaceTests", runId.ToString("N"));
            if (!string.Equals(Path.GetFullPath(local.Path), Path.GetFullPath(expected), StringComparison.OrdinalIgnoreCase))
                throw new IOException("The test data root escaped its explicitly owned directory.");
            return new TestApplicationData(local, temporary);
        }).GetAwaiter().GetResult();
        File.WriteAllText(Path.Combine(Current.LocalFolder.Path, "storage-boundary.txt"),
            "Run=" + RunId.ToString("N") + Environment.NewLine + "LocalFolder=" + Current.LocalFolder.Path);
        _originalFileGrants = StorageApplicationPermissions.FutureAccessList.Entries
            .ToDictionary(entry => entry.Token, entry => entry.Metadata, StringComparer.Ordinal);
    }

    public static void ValidatePreservedFileGrants()
    {
        var current = StorageApplicationPermissions.FutureAccessList.Entries
            .ToDictionary(entry => entry.Token, entry => entry.Metadata, StringComparer.Ordinal);
        foreach (var entry in _originalFileGrants)
            if (!current.TryGetValue(entry.Key, out var metadata) || metadata != entry.Value)
                throw new InvalidOperationException("The suite changed a pre-existing file grant: " + entry.Key);
    }
}
