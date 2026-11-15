// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading;
using System.Threading.Tasks;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Notepads.Infrastructure.DocumentAssets;

internal static partial class DocumentAssetClient
{
    // Only the source-linked App test host supplies this partial implementation.
    // Product builds erase the optional call and retain the real broker boundary.
    internal static Func<StorageFile, string, ulong, CancellationToken, Task<IRandomAccessStream>> TestRead;
    internal static Func<string, CancellationToken, Task> TestLaunch;
    internal static SessionChanges TestSessionChanges => _changes;
    internal static bool TestHasActiveSession => _directory != null || _changes != null;

    internal static Task<IRandomAccessStream> TestReadBrokerAsync(StorageFile document, string relative,
        ulong maximumBytes, CancellationToken cancellation) =>
        ReadDocumentAssetCoreAsync(document, relative, maximumBytes, cancellation);

    internal static async Task TestRetireSessionAsync(CancellationToken cancellation)
    {
        await Gate.WaitAsync(cancellation);
        try
        {
            CloseSession();
            await WaitForRetirementAsync(cancellation);
        }
        finally { Gate.Release(); }
    }

    internal static async Task TestWaitForRetirementAsync(string directory, CancellationToken cancellation, bool connected = false)
    {
        await Gate.WaitAsync(cancellation);
        try
        {
            if (_directory != null || _retiringSession.Directory != null)
                throw new InvalidOperationException("The retirement fixture needs an unused broker session.");
            _retiringSession = (directory, connected);
            try { await WaitForRetirementAsync(cancellation); }
            finally { _retiringSession = default; }
        }
        finally { Gate.Release(); }
    }

    static partial void OverrideReadForTest(StorageFile document, string relative, ulong maximumBytes,
        CancellationToken cancellation, ref Task<IRandomAccessStream> read) =>
        read = TestRead?.Invoke(document, relative, maximumBytes, cancellation);

    static partial void OverrideLaunchForTest(string directory, CancellationToken cancellation, ref Task launch) =>
        launch = TestLaunch?.Invoke(directory, cancellation);
}
