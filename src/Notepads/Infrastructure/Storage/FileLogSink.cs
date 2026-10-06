// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;

namespace Notepads.Infrastructure.Storage;

internal sealed class FileLogSink : ILogSink
{
    public StorageFile File { get; private set; }

    public async Task InitializeAsync()
    {
        if (File != null) return;
        var folder = await ApplicationData.Current.LocalFolder.CreateFolderAsync("Logs", CreationCollisionOption.OpenIfExists);
        File = await folder.CreateFileAsync(DateTime.UtcNow.ToString("yyyyMMddTHHmmss", CultureInfo.InvariantCulture) + ".log",
            CreationCollisionOption.ReplaceExisting);
    }

    public async Task AppendAsync(IEnumerable<string> messages)
    {
        await FileIO.AppendLinesAsync(File, messages);
    }
}
