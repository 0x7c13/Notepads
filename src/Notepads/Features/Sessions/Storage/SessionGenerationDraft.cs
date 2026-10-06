// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Windows.Storage.AccessCache;

namespace Notepads.Features.Sessions.Storage;

/// <summary>
/// Owns newly granted file permissions until their recovery manifest commits.
/// Previously committed access tokens must never be registered here.
/// </summary>
internal sealed class SessionGenerationDraft
{
    private readonly HashSet<string> _accessTokens = new(StringComparer.Ordinal);
    private bool _completed;
    private bool _retained;

    public void RegisterAccessToken(string token)
    {
        if (_completed) throw new InvalidOperationException("This session generation is already complete.");
        _accessTokens.Add(token);
    }

    public void Commit()
    {
        if (_completed) throw new InvalidOperationException("This session generation is already complete.");
        _completed = true;
        _accessTokens.Clear();
    }

    // Once a final publication is possible, grants are recovery resources.
    // An uncertain acknowledgement cannot authorize their rollback.
    public void PreserveForRecovery() => _retained = true;

    public async Task<IReadOnlyList<Exception>> RollbackAsync()
    {
        var failures = new List<Exception>();
        if (_completed || _retained) return failures;
        _completed = true;

        foreach (var token in _accessTokens)
        {
            try
            {
                if (StorageApplicationPermissions.FutureAccessList.ContainsItem(token))
                {
                    StorageApplicationPermissions.FutureAccessList.Remove(token);
                }
            }
            catch (Exception ex)
            {
                failures.Add(ex);
            }
        }

        _accessTokens.Clear();
        return failures;
    }
}
