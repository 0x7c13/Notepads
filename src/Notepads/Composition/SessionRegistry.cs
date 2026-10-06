// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Infrastructure.Diagnostics;

namespace Notepads.Composition;

internal sealed class SessionRegistry
{
    private readonly ConcurrentDictionary<ISessionPersistenceParticipant, Registration> _participants =
        new();

    public IDisposable Register(ISessionPersistenceParticipant participant)
    {
        if (participant == null) throw new ArgumentNullException(nameof(participant));
        var registration = new Registration(this, participant);
        if (!_participants.TryAdd(participant, registration))
            throw new InvalidOperationException("The session participant is already registered.");
        return registration;
    }

    public async Task SaveBeforeSuspensionAsync(CancellationToken cancellation)
    {
        var saves = new List<Task>();
        foreach (var participant in _participants.Keys)
            if (participant.IsBackupEnabled) saves.Add(SaveParticipantAsync(participant, cancellation));
        await Task.WhenAll(saves);
    }

    private static async Task SaveParticipantAsync(ISessionPersistenceParticipant participant, CancellationToken cancellation)
    {
        try
        {
            if (!await participant.SaveForSuspensionAsync(cancellation))
                LoggingService.LogError("[SessionRegistry] The last committed session was retained before suspension.");
        }
        catch (OperationCanceledException)
        {
            LoggingService.LogInfo("[SessionRegistry] Suspension deadline reached; the last committed session remains valid.");
        }
        catch (Exception ex)
        {
            LoggingService.LogError($"[SessionRegistry] Failed to prepare recovery before suspension: {ex.Message}");
        }
    }

    private sealed class Registration : IDisposable
    {
        private readonly SessionRegistry _owner;
        private readonly ISessionPersistenceParticipant _participant;

        public Registration(SessionRegistry owner, ISessionPersistenceParticipant participant)
        {
            _owner = owner;
            _participant = participant;
        }

        public void Dispose()
        {
            ((ICollection<KeyValuePair<ISessionPersistenceParticipant, Registration>>)_owner._participants)
                .Remove(new KeyValuePair<ISessionPersistenceParticipant, Registration>(_participant, this));
        }
    }
}
