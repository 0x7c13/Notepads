// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Contracts.Legacy;
using Notepads.Features.Sessions.Contracts.Recovery;
using Notepads.Features.Sessions.Validation;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

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
    private readonly RecoveryStamp _expectedStamp;
    private readonly RecoverySourceLifetime _lifetime;
    private readonly RecoveryStamp _targetStamp;
    private readonly Lazy<Task<StorageFile>> _editingFile;
    private readonly Lazy<Task<StorageFile>>[] _fallbackEditingFiles;

    internal RecoveryDocumentSource(RecoveryDocumentDecision decision, TextEditorSessionDataV1 legacy,
        bool isOwned, RecoveryStamp expectedStamp, RecoveryAdoptionOrigin origin,
        RecoverySourceLifetime lifetime, RecoveryStamp targetStamp = null, TextEditorSessionDataV2 descriptorOverride = null)
    {
        if ((decision == null) == (legacy == null))
            throw new ArgumentException("A recovery source requires exactly one current or legacy document.");
        if (origin?.Decision != null && !ReferenceEquals(decision, origin.Decision))
            throw new ArgumentException("The adoption origin must retain this source's recovery decision.");
        Decision = decision;
        _descriptor = descriptorOverride == null ? decision?.Descriptor : SessionRecoveryCatalog.CloneEditor(descriptorOverride);
        _legacy = CloneLegacy(legacy);
        _expectedStamp = SessionRecoveryAuthority.CopyStamp(expectedStamp);
        _targetStamp = SessionRecoveryAuthority.CopyStamp(targetStamp ?? expectedStamp);
        IsOwned = isOwned;
        Origin = origin;
        _lifetime = lifetime;
        _editingFile = CreateEditingFileBinding(-1);
        _fallbackEditingFiles = Enumerable.Range(0, FallbackCount).Select(CreateEditingFileBinding).ToArray();
    }

    public Guid Id => _descriptor?.Id ?? _legacy.Id;
    public bool IsOwned { get; }
    public bool IsOwnScopeRescue => !IsOwned && Origin?.SourceStamp?.ScopeId == _targetStamp.ScopeId;
    public TextEditorSessionDataV2 Descriptor => _descriptor == null ? null : SessionRecoveryCatalog.CloneEditor(_descriptor);
    public TextEditorSessionDataV1 LegacyDescriptor => CloneLegacy(_legacy);
    public DocumentMetadata Metadata => (_descriptor?.StateMetaData ?? _legacy?.StateMetaData)?.Clone();
    public string EditingFileName => _descriptor?.EditingFileName ?? _legacy?.EditingFileName;
    public string EditingFilePath => _descriptor?.EditingFilePath ?? _legacy?.EditingFilePath;
    public string EditingFileFutureAccessToken => _descriptor?.EditingFileFutureAccessToken ?? _legacy?.EditingFileFutureAccessToken;
    public RecoveryStamp ExpectedStamp => SessionRecoveryAuthority.CopyStamp(_expectedStamp);
    public RecoveryStamp TargetStamp => SessionRecoveryAuthority.CopyStamp(_targetStamp);
    public int FallbackCount => Decision?.Fallbacks.Count ?? 0;
    internal RecoveryDocumentDecision Decision { get; }
    internal RecoveryAdoptionOrigin Origin { get; }

    internal void RequireAvailable() => ObjectDisposedException.ThrowIf(_lifetime.Disposed, this);

    internal Task<StorageFile> ResolveEditingFileAsync(int fallbackIndex = -1)
    {
        RequireAvailable();
        return (fallbackIndex < 0 ? _editingFile : _fallbackEditingFiles[fallbackIndex]).Value;
    }

    internal string GetEditingFilePath(int fallbackIndex = -1)
    {
        RequireAvailable();
        return fallbackIndex < 0 && _legacy != null ? _legacy.EditingFilePath : GetDescriptor(fallbackIndex)?.EditingFilePath;
    }

    private Lazy<Task<StorageFile>> CreateEditingFileBinding(int fallbackIndex) => new(() =>
    {
        var token = fallbackIndex < 0 && _legacy != null ? _legacy.EditingFileFutureAccessToken :
            GetDescriptor(fallbackIndex)?.EditingFileFutureAccessToken;
        // Keep a missing grant detached for this source lifetime. Each caller cancels only its wait.
        return token == null ? Task.FromResult<StorageFile>(null) :
            FutureAccessListUtility.GetFileFromFutureAccessListAsync(token);
    }, LazyThreadSafetyMode.ExecutionAndPublication);

    internal TextEditorSessionDataV2 GetDescriptor(int fallbackIndex)
    {
        RequireAvailable();
        var data = fallbackIndex < 0 ? Descriptor : Decision.Fallbacks[fallbackIndex].Descriptor;
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
    private readonly RecoveryStamp _sourceStamp;

    public RecoveryAdoptionOrigin(string legacyFingerprint) => LegacyFingerprint = legacyFingerprint;

    public RecoveryAdoptionOrigin(RecoveryStamp sourceStamp, RecoveryDocumentDecision decision,
        RecoveryAddress rootAddress, string rootSha256)
    {
        ArgumentNullException.ThrowIfNull(sourceStamp);
        ArgumentNullException.ThrowIfNull(decision);
        _sourceStamp = SessionRecoveryAuthority.CopyStamp(sourceStamp);
        Decision = decision;
        RootAddress = rootAddress;
        RootSha256 = rootSha256;
    }

    public string LegacyFingerprint { get; }
    public RecoveryStamp SourceStamp => SessionRecoveryAuthority.CopyStamp(_sourceStamp);
    public RecoveryDocumentDecision Decision { get; }
    public RecoveryAddress RootAddress { get; }
    public string RootSha256 { get; }
    public TextEditorSessionDataV2 SourceEditor => Decision?.Descriptor;
    public long ConsumptionCutoff => Decision?.ConsumptionCutoff ?? 0;
}
