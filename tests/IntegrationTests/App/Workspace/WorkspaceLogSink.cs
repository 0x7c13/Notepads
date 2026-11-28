// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;

namespace NotepadsWorkspaceTests;

internal sealed class WorkspaceLogSink : ILogSink
{
    public StorageFile File { get; private set; }
    public async Task InitializeAsync() => File = await ApplicationData.Current.LocalFolder.CreateFileAsync("workspace.log", CreationCollisionOption.OpenIfExists);
    public async Task AppendAsync(IEnumerable<string> messages) => await FileIO.AppendTextAsync(File, string.Join(Environment.NewLine, messages) + Environment.NewLine);
}
