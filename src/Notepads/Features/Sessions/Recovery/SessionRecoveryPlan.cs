// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Validation;

namespace Notepads.Features.Sessions.Recovery;

/// <summary>Frozen discovery results. The caller drains all readers before disposing these shared history pins.</summary>
internal sealed class SessionRecoveryPlan : IDisposable
{
    private readonly List<IDisposable> _pins;
    private readonly RecoveryStamp _expectedStamp;
    internal RecoverySourceLifetime Lifetime { get; }

    internal SessionRecoveryPlan(IEnumerable<RecoveryDocumentSource> sources, Guid? selectedEditorId,
        double tabScrollOffset, RecoveryStamp expectedStamp, List<IDisposable> pins, RecoverySourceLifetime lifetime)
    {
        Sources = Array.AsReadOnly(sources.ToArray());
        SelectedEditorId = selectedEditorId;
        TabScrollOffset = tabScrollOffset;
        _expectedStamp = SessionRecoveryAuthority.CopyStamp(expectedStamp);
        _pins = pins;
        Lifetime = lifetime;
    }

    public IReadOnlyList<RecoveryDocumentSource> Sources { get; }
    public Guid? SelectedEditorId { get; }
    public double TabScrollOffset { get; }
    public RecoveryStamp ExpectedStamp => SessionRecoveryAuthority.CopyStamp(_expectedStamp);
    public void Dispose()
    {
        if (Lifetime.Disposed) return;
        Lifetime.Disposed = true;
        foreach (var pin in _pins) pin.Dispose();
        _pins.Clear();
    }
}

internal sealed class RecoverySourceLifetime
{
    internal bool Disposed;
}

/// <summary>One immutable source, including the exact evidence required to adopt its content.</summary>
internal sealed class RecoveryDocumentSource
{
    private readonly TextEditorSessionDataV2 _descriptor;
    private readonly TextEditorSessionDataV1 _legacy;
    private readonly TextEditorSessionDataV2[] _fallbacks;
    private readonly RecoveryStamp _expectedStamp;
    private readonly RecoverySourceLifetime _lifetime;
    private readonly RecoveryStamp _targetStamp;

    internal RecoveryDocumentSource(TextEditorSessionDataV2 descriptor, TextEditorSessionDataV1 legacy,
        bool isOwned, RecoveryStamp expectedStamp, IEnumerable<TextEditorSessionDataV2> fallbacks,
        RecoveryAdoptionOrigin origin, RecoverySourceLifetime lifetime, RecoveryStamp targetStamp = null)
    {
        _descriptor = descriptor == null ? null : SessionRecoveryCatalog.CloneEditor(descriptor);
        _legacy = CloneLegacy(legacy);
        _fallbacks = (fallbacks ?? []).Select(SessionRecoveryCatalog.CloneEditor).ToArray();
        _expectedStamp = SessionRecoveryAuthority.CopyStamp(expectedStamp);
        _targetStamp = SessionRecoveryAuthority.CopyStamp(targetStamp ?? expectedStamp);
        IsOwned = isOwned;
        Origin = origin;
        _lifetime = lifetime;
    }

    public Guid Id => _descriptor?.Id ?? _legacy.Id;
    public bool IsOwned { get; }
    public TextEditorSessionDataV2 Descriptor => _descriptor == null ? null : SessionRecoveryCatalog.CloneEditor(_descriptor);
    public TextEditorSessionDataV1 LegacyDescriptor => CloneLegacy(_legacy);
    public DocumentMetadata Metadata => (_descriptor?.StateMetaData ?? _legacy?.StateMetaData)?.Clone();
    public string EditingFileName => _descriptor?.EditingFileName ?? _legacy?.EditingFileName;
    public string EditingFilePath => _descriptor?.EditingFilePath ?? _legacy?.EditingFilePath;
    public string EditingFileFutureAccessToken => _descriptor?.EditingFileFutureAccessToken ?? _legacy?.EditingFileFutureAccessToken;
    public RecoveryStamp ExpectedStamp => SessionRecoveryAuthority.CopyStamp(_expectedStamp);
    public RecoveryStamp TargetStamp => SessionRecoveryAuthority.CopyStamp(_targetStamp);
    public int FallbackCount => _fallbacks.Length;
    internal RecoveryAdoptionOrigin Origin { get; }

    internal void RequireAvailable() => ObjectDisposedException.ThrowIf(_lifetime.Disposed, this);

    internal TextEditorSessionDataV2 GetDescriptor(int fallbackIndex)
    {
        RequireAvailable();
        var data = fallbackIndex < 0 ? Descriptor : SessionRecoveryCatalog.CloneEditor(_fallbacks[fallbackIndex]);
        if (data != null && data.Id != Id)
        {
            data.Id = Id;
            data.EditingFileFutureAccessToken = null;
            data.EditingFilePath = null;
            data.StateMetaData.HasEditingFile = false;
            data.StateMetaData.RequiresSaveAs = true;
            data.StateMetaData.IsModified = true;
            data.TextDirty = true;
        }
        return data;
    }

    private static TextEditorSessionDataV1 CloneLegacy(TextEditorSessionDataV1 data) => data == null ? null : new()
    {
        Id = data.Id, LastSavedBackupFilePath = data.LastSavedBackupFilePath,
        PendingBackupFilePath = data.PendingBackupFilePath, BackupEncoding = data.BackupEncoding,
        EditingFileFutureAccessToken = data.EditingFileFutureAccessToken, EditingFileName = data.EditingFileName,
        EditingFilePath = data.EditingFilePath, StateMetaData = data.StateMetaData?.Clone()
    };
}

internal sealed class RecoveryAdoptionOrigin
{
    public string LegacyFingerprint { get; init; }
    public RecoveryStamp SourceStamp { get; init; }
    public RecoveryAddress RootAddress { get; init; }
    public string RootSha256 { get; init; }
    public TextEditorSessionDataV2 SourceEditor { get; init; }
    public long ConsumptionCutoff { get; init; }
    public IReadOnlyList<TextEditorSessionDataV2> UnselectedNewerDescriptors { get; init; } = [];
}
