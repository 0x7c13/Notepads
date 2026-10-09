// Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE.
using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Recovery;
using Notepads.Presentation.Controls.TextEditor;

namespace NotepadsWorkspaceTests;

internal sealed class RestorePreparationGate
{
    private readonly bool _enabled;
    private readonly Guid _initialSelection;
    private readonly Dictionary<Guid, TaskCompletionSource<object>> _permits = [];
    public readonly List<Guid> Admitted = [];
    private bool _released;
    private Guid _heldPromotion;
    private readonly TaskCompletionSource<object> _promotionReached = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private readonly TaskCompletionSource<object> _promotionPermit = new(TaskCreationOptions.RunContinuationsAsynchronously);
    internal Task PromotionReached => _promotionReached.Task;
    internal ITextEditor PromotionCandidate { get; private set; }
    internal bool PromotionGateReturned { get; private set; }

    internal RestorePreparationGate(bool enabled, Guid initialSelection)
    {
        _enabled = enabled;
        _initialSelection = initialSelection;
    }

    internal async Task BeforePreparationAsync(RecoveryDocumentSource source, CancellationToken cancellation)
    {
        Admitted.Add(source.Id);
        if (!_enabled || _released || source.Id == _initialSelection) return;
        await GetPermit(source.Id).Task.WaitAsync(cancellation);
    }

    internal void Release(Guid id) => GetPermit(id).TrySetResult(null);
    internal void HoldPromotion(Guid id) => _heldPromotion = id;
    internal void ReleasePromotion() => _promotionPermit.TrySetResult(null);
    internal async Task BeforePromotionAsync(RecoveryDocumentSource source, ITextEditor candidate, CancellationToken cancellation)
    {
        if (source.Id != _heldPromotion) return;
        PromotionCandidate = candidate;
        _promotionReached.TrySetResult(null);
        await _promotionPermit.Task.WaitAsync(cancellation);
        PromotionGateReturned = true;
    }
    internal void ReleaseAll()
    {
        _released = true;
        foreach (var permit in _permits.Values) permit.TrySetResult(null);
    }

    private TaskCompletionSource<object> GetPermit(Guid id)
    {
        if (!_permits.TryGetValue(id, out var permit))
            _permits.Add(id, permit = new(TaskCreationOptions.RunContinuationsAsynchronously));
        return permit;
    }
}
