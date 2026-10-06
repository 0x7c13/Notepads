// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;
using System.Threading.Tasks;
using Windows.Storage;

namespace Notepads.Infrastructure.Diagnostics;

internal interface ILogSink
{
    StorageFile File { get; }

    Task InitializeAsync();

    Task AppendAsync(IEnumerable<string> messages);
}
