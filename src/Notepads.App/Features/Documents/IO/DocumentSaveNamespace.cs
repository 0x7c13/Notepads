// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
using Notepads.Infrastructure.Diagnostics;
using Windows.Storage;

namespace Notepads.Features.Documents.IO;

internal static partial class DocumentSaveTarget
{
    // Keep the picker/provider grant for I/O. A local name that passed through
    // a junction or symbolic link must continue to reach the leased directory entry.
    internal static IDisposable Pin(StorageFile file, string expectedFinalPath, bool renamingEntry = false) =>
        Pin(file, expectedFinalPath, out _, renamingEntry);

    internal static IDisposable Pin(StorageFile file, string expectedFinalPath, out string admittedSpelling, bool renamingEntry = false)
    {
        admittedSpelling = null;
        if (expectedFinalPath == null) return null;
        IDisposable protection = null;
        try
        {
            var path = file.Path;
            if (string.IsNullOrEmpty(path))
            {
                VerifyPinnedLocation(file, expectedFinalPath, grantOnly: true);
                return null;
            }
            var nativePath = ExtendedPath(path);
            using var native = OpenMetadataByName(nativePath);
            // The package can see FILE_NOT_FOUND for a file outside its direct namespace.
            // The original grant must still open now and identify the expected entry.
            using var granted = native.IsInvalid ? OpenGrantedMetadata(file) : null;
            var handle = granted ?? native;
            var current = GetFinalPath(handle);
            if (current == null) throw new DocumentSaveException(DocumentSaveStage.Coordinate);
            if (!SameLocationOnWorker(expectedFinalPath, current))
                throw new DocumentSaveException(DocumentSaveStage.Conflict);
            var dosPath = GetFinalPath(handle, 0);
            OnCheckingNormalizedDosPath(path, ref dosPath);
            bool ordinarySpelling = string.Equals(nativePath, dosPath, StringComparison.OrdinalIgnoreCase);
            bool isLocal = IsLocalPath(nativePath);
            if (isLocal && nativePath == dosPath && granted == null)
                protection = NamespacePins.TryPinOrdinaryParent(nativePath);
            else if (isLocal && (!ordinarySpelling || granted == null))
            {
                var pins = new NamespacePins();
                protection = pins;
                try { pins.Protect(nativePath, renamingEntry); }
                catch (Exception ex) when (ex.HResult == unchecked((int)0x80070005))
                {
                    if (!ordinarySpelling && (pins.ObservedReparsePoint ||
                        !MatchesPhysicalDrivePath(nativePath, current, StringComparison.OrdinalIgnoreCase, dosPath == null)))
                    {
                        LoggingService.LogError($"[Save] Cannot inspect route '{nativePath}'; final '{current}', DOS '{dosPath}', brokered={granted != null}.");
                        throw;
                    }
                    // A file grant may allow direct file metadata but deny access to its ancestors.
                    ordinarySpelling = true;
                    protection.Dispose();
                    protection = null;
                }
            }
            if (!renamingEntry && ordinarySpelling)
            {
                // This exact grant opened the selected spelling and Windows identified its entry.
                // Its own transaction can adopt that leaf case. Keep the exact physical parent.
                var name = Path.GetFileName(path);
                var selected = Path.Combine(Path.GetDirectoryName(current), name);
                if (expectedFinalPath == current && file.Name == name && CompareSpelling(current, selected) == null)
                    admittedSpelling = selected;
            }
            // A verified parent pins the entry's location. A file-only grant must freshly
            // establish that location itself, including immediately before an editor rename.
            if (isLocal) VerifyPinnedLocation(file, expectedFinalPath, grantOnly: protection == null);
            return protection;
        }
        catch (Exception ex)
        {
            protection?.Dispose();
            if (ex is not DocumentSaveException && (IsNamespaceError(ex) || ex is ArgumentException))
                throw new DocumentSaveException((ex.HResult & 0xffff) is 2 or 3
                    ? DocumentSaveStage.Conflict : DocumentSaveStage.Coordinate, ex);
            throw;
        }
    }

    private static void VerifyPinnedLocation(StorageFile file, string expected, bool grantOnly)
    {
        if (!grantOnly && !SameLocationOnWorker(expected, GetFinalPath(file)))
            throw new DocumentSaveException(DocumentSaveStage.Conflict);
        using var granted = OpenGrantedMetadata(file);
        var observed = GetFinalPath(granted);
        OnRecheckingPinnedLocation(ref observed);
        if (!SameLocationOnWorker(expected, observed)) throw new DocumentSaveException(DocumentSaveStage.Conflict);
    }

    static partial void OnRecheckingPinnedLocation(ref string observed);
    static partial void OnCheckingNormalizedDosPath(string path, ref string dosPath);
    static partial void OnCheckingOrdinaryParent(ref bool hasTag, ref string finalPath);
    static partial void OnCheckingPinnedAnchor(ref string finalPath);

    private static DocumentSaveException NamespaceFailure(int error) =>
        error is 2 or 3 ? new DocumentSaveException(DocumentSaveStage.Conflict) :
        new DocumentSaveException(DocumentSaveStage.Coordinate, Marshal.GetExceptionForHR(unchecked((int)0x80070000) | error));

    private static bool IsNamespaceError(Exception ex) => DocumentSaveException.IsStorageError(ex) ||
        ex is NotSupportedException or NotImplementedException or System.Security.SecurityException;

    private static string ExtendedPath(string path) => path.StartsWith(@"\\?\", StringComparison.Ordinal) ? path :
        path.StartsWith(@"\\", StringComparison.Ordinal) ? @"\\?\UNC\" + path[2..] : @"\\?\" + path;

    private const string VolumeGuidPrefix = @"\\?\Volume{";
    private const string NtVolumePrefix = @"\Device\HarddiskVolume";

    private static bool IsLocalPath(string path) => path.StartsWith(VolumeGuidPrefix, StringComparison.OrdinalIgnoreCase) ||
        (path.Length > 6 && path[5] == ':' && path[6] == '\\');

    private static int LocalVolumeSeparator(string path)
    {
        int nameStart = path.StartsWith(VolumeGuidPrefix, StringComparison.OrdinalIgnoreCase) ? VolumeGuidPrefix.Length :
            path.StartsWith(NtVolumePrefix, StringComparison.OrdinalIgnoreCase) ? NtVolumePrefix.Length : 0;
        return nameStart == 0 ? -1 : path.IndexOf('\\', nameStart);
    }

    private static string OpenableLocalVolumePath(string path) =>
        path.StartsWith(NtVolumePrefix, StringComparison.OrdinalIgnoreCase) && LocalVolumeSeparator(path) >= 0 ?
            @"\\?\GLOBALROOT" + path : path;

    private static uint CanonicalVolumeFormat(string path) =>
        path.StartsWith(NtVolumePrefix, StringComparison.OrdinalIgnoreCase) ? 2u : 1u;

    private static bool MatchesPhysicalDrivePath(string path, string finalPath, StringComparison comparison,
        bool allowVolumeSpellingFallback = true)
    {
        if (finalPath == null || path.Length <= 7 || path[5] != ':' || path[6] != '\\') return false;
        using var drive = OpenMetadataByName(path[..7], 0x02000000 /* FILE_FLAG_BACKUP_SEMANTICS */);
        var finalRoot = GetFinalPath(drive);
        if (finalRoot != null)
            return string.Equals(finalRoot.TrimEnd('\\') + path[6..], finalPath, comparison);

        // Some app containers can open the grant but cannot query the drive
        // mapping. Keep requiring the normalized name below the local volume.
        // The grant's full volume-qualified location is checked independently.
        if (!allowVolumeSpellingFallback) return false;
        int suffix = LocalVolumeSeparator(finalPath);
        return suffix >= 0 && path.AsSpan(6).Equals(finalPath.AsSpan(suffix), comparison);
    }

    private static bool ResolvesParentCaseSpelling(string path, string spelling)
    {
        // A leaf-only no-follow open still follows intermediate links. For a
        // parent spelling change, prove each changed parent without following it.
        // Network providers and inaccessible parents retain their existing refusal.
        int rootEnd = LocalVolumeSeparator(path);
        if (rootEnd < 0 || rootEnd != LocalVolumeSeparator(spelling) ||
            !path.AsSpan(0, rootEnd).SequenceEqual(spelling.AsSpan(0, rootEnd))) return false;
        OnProvingCaseSpelling(grant: false);
        int parentEnd = path.LastIndexOf('\\');
        int difference = 0;
        while (difference < parentEnd && path[difference] == spelling[difference]) difference++;
        if (difference == parentEnd) return false;
        int boundary = path.LastIndexOf('\\', difference);
        var root = path[..(rootEnd + 1)];
        var anchor = boundary < root.Length ? root : path[..boundary];
        using var pins = new NamespacePins();
        // The volume root itself cannot be renamed and may not be accessible to
        // a file/folder grant. Each pinned suffix must still name that exact root.
        bool hasAnchor = anchor != root;
        if (hasAnchor && !pins.TryPinCaseParent(anchor, anchor)) return false;
        int start = anchor.EndsWith('\\') ? anchor.Length : anchor.Length + 1;
        for (int end = path.IndexOf('\\', start); end >= 0; end = path.IndexOf('\\', end + 1))
        {
            if (!pins.TryPinCaseParent(path[..end], spelling[..end])) return false;
        }
        using var leaf = OpenMetadataByName(path, NamespacePins.OpenReparsePoint);
        return GetFinalPath(leaf, CanonicalVolumeFormat(spelling)) == spelling &&
            (!hasAnchor || pins.HasCaseParentSpelling(anchor, anchor));
    }

    private sealed partial class NamespacePins : IDisposable
    {
        internal const uint OpenReparsePoint = 0x00200000;
        private const uint BackupSemantics = 0x02000000;
        private const uint Traverse = 0x20;
        private const uint ReadData = 1;
        private const uint NameSurrogate = 0x20000000;
        private readonly Dictionary<string, SafeFileHandle> _handles = new(StringComparer.Ordinal);

        internal bool ObservedReparsePoint { get; private set; }

        internal bool TryPinCaseParent(string path, string spelling)
        {
            bool unavailable = false;
            OnOpeningNamedMetadata(path, ref unavailable);
            if (unavailable) return false;
            var handle = CreateFileFromAppW(OpenableLocalVolumePath(path), Traverse | 0x80 /* FILE_READ_ATTRIBUTES */, FileShare.ReadWrite,
                IntPtr.Zero, FileMode.Open, OpenReparsePoint | BackupSemantics, IntPtr.Zero);
            if (handle.IsInvalid) { handle.Dispose(); return false; }
            _handles.Add(path, handle);
            return HasCaseParentSpelling(path, spelling);
        }

        internal bool HasCaseParentSpelling(string path, string spelling) =>
            GetFileInformationByHandleEx(_handles[path], 9 /* FileAttributeTagInfo */, out var info, 8) &&
            (info.FileAttributes & 0x10 /* FILE_ATTRIBUTE_DIRECTORY */) != 0 &&
            (info.ReparseTag & NameSurrogate) == 0 &&
            GetFinalPath(_handles[path], CanonicalVolumeFormat(spelling)) == spelling;

        private static string GetDirectorySpelling(SafeFileHandle handle, string path)
        {
            if (path.StartsWith(VolumeGuidPrefix, StringComparison.OrdinalIgnoreCase))
                return GetFinalPath(handle, 1);
            var observed = GetFinalPath(handle, 0);
            OnCheckingNormalizedDosPath(path, ref observed);
            if (observed != null) return observed;

            // A no-follow directory handle can still prove the anchor when
            // Windows supplies only its volume-qualified name.
            var physicalPath = GetFinalPath(handle);
            return MatchesPhysicalDrivePath(path, physicalPath, StringComparison.Ordinal) ? path : physicalPath;
        }

        private static string FindAnchor(string path, string root)
        {
            // Query upward from the granted subtree. Even a cross-volume link may sit inside
            // an accessible folder whose ancestors the AppContainer cannot enumerate.
            var parent = Path.GetDirectoryName(path);
            while (parent != null && parent != root)
            {
                using var handle = CreateFileFromAppW(parent, 0, FileShare.ReadWrite | FileShare.Delete,
                    IntPtr.Zero, FileMode.Open, BackupSemantics | OpenReparsePoint, IntPtr.Zero);
                if (!handle.IsInvalid && GetFileInformationByHandleEx(handle, 9, out var info, 8) &&
                    (info.ReparseTag & NameSurrogate) == 0 && GetDirectorySpelling(handle, parent) == parent)
                    return parent;
                parent = Path.GetDirectoryName(parent);
            }
            return root;
        }

        internal static SafeFileHandle TryPinOrdinaryParent(string path)
        {
            var parent = Path.GetDirectoryName(path);
            if (parent == null) throw new DocumentSaveException(DocumentSaveStage.Coordinate);
            var handle = CreateFileFromAppW(parent, Traverse | 0x80 /* FILE_READ_ATTRIBUTES */, FileShare.ReadWrite, IntPtr.Zero,
                FileMode.Open, BackupSemantics | OpenReparsePoint, IntPtr.Zero);
            if (handle.IsInvalid)
            {
                // The picker can grant a file without granting its directory. The writer
                // still verifies the location inside the transaction in that case.
                var error = Marshal.GetLastPInvokeError();
                handle.Dispose();
                if (error == 5) return null;
                throw NamespaceFailure(error);
            }
            try
            {
                var observed = GetDirectorySpelling(handle, parent);
                var hasTag = GetFileInformationByHandleEx(handle, 9, out var info, 8);
                OnCheckingOrdinaryParent(ref hasTag, ref observed);
                if (!hasTag || observed == null) throw new DocumentSaveException(DocumentSaveStage.Coordinate);
                if ((info.ReparseTag & NameSurrogate) != 0 || observed != parent)
                    throw new DocumentSaveException(DocumentSaveStage.Conflict);
                return handle;
            }
            catch
            {
                handle.Dispose();
                throw;
            }
        }

        internal void Protect(string path, bool renamingEntry = false)
        {
            // Follow immediate (including relative) targets one hop at a time. Asking
            // only for the ultimate target would leave intermediate junctions mutable.
            for (int hop = 0; hop <= 63; hop++)
            {
                var root = Path.GetPathRoot(path);
                // The server owns a UNC suffix just as it does for a direct UNC grant.
                // Keep the local links already pinned until the original grant's transaction is closed.
                if (path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase)) return;
                if (string.IsNullOrEmpty(root) || !IsLocalPath(path))
                    throw new DocumentSaveException(DocumentSaveStage.Coordinate);
                var prefix = FindAnchor(path, root);
                if (prefix != root)
                {
                    if (ProtectComponent(prefix, leaf: false) != null)
                        throw new DocumentSaveException(DocumentSaveStage.Conflict);
                    var observed = GetDirectorySpelling(_handles[prefix], prefix);
                    OnCheckingPinnedAnchor(ref observed);
                    if (observed == null) throw new DocumentSaveException(DocumentSaveStage.Coordinate);
                    if (observed != prefix) throw new DocumentSaveException(DocumentSaveStage.Conflict);
                }
                bool followed = false;
                var components = path[prefix.Length..].Split('\\', StringSplitOptions.RemoveEmptyEntries);
                for (int i = 0; i < components.Length; i++)
                {
                    prefix = Path.Combine(prefix, components[i]);
                    var leaf = i == components.Length - 1;
                    // StorageFile.RenameAsync moves the entry itself, including a file symlink.
                    // Its ancestors stay pinned, but that entry must permit the rename.
                    if (leaf && renamingEntry) return;
                    var target = ProtectComponent(prefix, leaf);
                    if (target == null) continue;
                    var parent = Path.GetDirectoryName(prefix);
                    path = ExtendedPath(Path.GetFullPath(target, parent));
                    for (int j = i + 1; j < components.Length; j++) path = Path.Combine(path, components[j]);
                    followed = true;
                    break;
                }
                if (!followed) return;
            }
            throw new DocumentSaveException(DocumentSaveStage.Coordinate);
        }

        private string ProtectComponent(string path, bool leaf)
        {
            var attrs = File.GetAttributes(path);
            var reparse = (attrs & System.IO.FileAttributes.ReparsePoint) != 0;
            ObservedReparsePoint |= reparse;
            if (leaf && !reparse) return null;
            var folder = (attrs & System.IO.FileAttributes.Directory) != 0;
            if (!_handles.TryGetValue(path, out var handle))
            {
                // FILE_TRAVERSE / FILE_READ_DATA participates in Windows share checks.
                // A zero desired-access metadata handle does not protect against replacement.
                handle = CreateFileFromAppW(ExtendedPath(path), (folder ? Traverse : ReadData) | 0x80 /* FILE_READ_ATTRIBUTES */,
                    reparse ? FileShare.Read : FileShare.ReadWrite, IntPtr.Zero, FileMode.Open,
                    OpenReparsePoint | BackupSemantics, IntPtr.Zero);
                if (handle.IsInvalid)
                {
                    var error = Marshal.GetLastPInvokeError();
                    handle.Dispose();
                    throw NamespaceFailure(error);
                }
                _handles.Add(path, handle);
            }
            if (!GetFileInformationByHandleEx(handle, 9 /* FileAttributeTagInfo */, out var info, 8))
                throw NamespaceFailure(Marshal.GetLastPInvokeError());
            if ((info.ReparseTag & NameSurrogate) == 0)
            {
                // Cloud placeholders and other ordinary leaves must remain replaceable.
                if (leaf) { _handles.Remove(path); handle.Dispose(); }
                return null;
            }
            if (!reparse) throw new DocumentSaveException(DocumentSaveStage.Conflict);
            // OpenTransactedWriteAsync rejects a file symlink. Report the platform's
            // error before our no-delete handle can obscure it with a sharing violation.
            if (leaf && !folder && info.ReparseTag == 0xA000000C /* IO_REPARSE_TAG_SYMLINK */)
                throw new DocumentSaveException(DocumentSaveStage.OpenTarget,
                    Marshal.GetExceptionForHR(unchecked((int)0x800705B8))); // ERROR_SYMLINK_NOT_SUPPORTED
            FileSystemInfo link = folder ? new DirectoryInfo(path) : new FileInfo(path);
            return link.LinkTarget ?? throw new DocumentSaveException(DocumentSaveStage.Coordinate);
        }

        public void Dispose()
        {
            foreach (var handle in _handles.Values) handle.Dispose();
            _handles.Clear();
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct FileAttributeTagInfo
    {
        public uint FileAttributes;
        public uint ReparseTag;
    }

    [LibraryImport("api-ms-win-core-file-l1-2-0.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GetFileInformationByHandleEx(SafeFileHandle file, int infoClass, out FileAttributeTagInfo info, uint size);
}
