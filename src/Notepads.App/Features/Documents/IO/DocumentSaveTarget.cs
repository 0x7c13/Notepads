// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Buffers;
using System.IO;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;
using Notepads.Features.Documents.Contracts;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Storage;
using Notepads.Infrastructure.Threading;
using Windows.Storage;
using Windows.Storage.Streams;
using WinRT;
using WinRT.Interop;

namespace Notepads.Features.Documents.IO;

internal static partial class DocumentSaveTarget
{
    private const int VerificationBufferSize = 256 * 1024;
    private static readonly string EmptyFileSha256 = Convert.ToHexString(SHA256.HashData(ReadOnlySpan<byte>.Empty));

    internal sealed record Identity(StorageFile File, string FinalPath)
    {
        // The grant remains with its StorageFile. The normalized directory-entry name
        // survives atomic replacement and is shared by comparison and coordination.
        internal async ValueTask<bool?> MatchesAsync(Identity other)
        {
            if (other == null) return false;
            if (FinalPath != null && other.FinalPath != null)
                return await SameLocationAsync(FinalPath, other.FinalPath).ConfigureAwait(false);
            if (ReferenceEquals(File, other.File)) return true;
            return !NeedsFileCaseProof(File, other.File) &&
                await Task.Run(() => File.IsEqual(other.File)).ConfigureAwait(false) ? true : null;
        }
    }

    internal static async Task<Identity> ResolveAsync(StorageFile file, CancellationToken cancellation)
    {
        if (file == null) return null;
        try
        {
            return new Identity(file, await Task.Run(() => GetFinalPath(file), cancellation).ConfigureAwait(false));
        }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex))
        {
            throw new DocumentSaveException(DocumentSaveStage.Coordinate, ex);
        }
    }

    private static bool? CompareSpelling(string expected, string observed)
    {
        if (string.Equals(expected, observed, StringComparison.Ordinal)) return true;
        if (!string.Equals(expected, observed, StringComparison.OrdinalIgnoreCase)) return false;
        // A casing difference needs filesystem proof. Parent components require
        // their own no-follow proof before a leaf open can establish equivalence.
        if (expected.LastIndexOf('\\') < 0) return false;
        return null;
    }

    private static bool ProveCaseEquivalence(string first, string second)
    {
        int leaf = first.LastIndexOf('\\') + 1;
        if (!first.AsSpan(0, leaf).SequenceEqual(second.AsSpan(0, leaf)))
            return ResolvesParentCaseSpelling(first, second) || ResolvesParentCaseSpelling(second, first);
        return ResolvesToSpelling(first, second) || ResolvesToSpelling(second, first);
    }

    private static bool SameLocationOnWorker(string expected, string observed) =>
        CompareSpelling(expected, observed) ?? ProveCaseEquivalence(expected, observed);

    internal static ValueTask<bool> SameLocationAsync(string expected, string observed)
    {
        if (CompareSpelling(expected, observed) is { } match) return ValueTask.FromResult(match);
        return new ValueTask<bool>(Task.Run(() => ProveCaseEquivalence(expected, observed)));
    }

    private static bool ResolvesToSpelling(string path, string spelling)
    {
        const string networkPrefix = @"\Device\Mup\";
        bool network = path.StartsWith(networkPrefix, StringComparison.Ordinal);
        if (network) path = @"\\?\UNC\" + path[networkPrefix.Length..];
        else if (LocalVolumeSeparator(path) < 0) return false;
        OnProvingCaseSpelling(grant: false);
        using var handle = OpenMetadataByName(path, 0x00200000 /* FILE_FLAG_OPEN_REPARSE_POINT */);
        var observed = network ? GetFinalPath(handle) : GetFinalPath(handle, CanonicalVolumeFormat(spelling));
        return observed == spelling;
    }

    internal static ValueTask<bool> IsGrantSpellingChangeAsync(StorageFile file, string previousPath, string previousName,
        string targetPath, string admittedSpelling)
    {
        if (targetPath == null || string.IsNullOrEmpty(previousPath)) return ValueTask.FromResult(false);
        var path = file.Path;
        var name = file.Name;
        if (!string.Equals(previousPath, path, StringComparison.OrdinalIgnoreCase) ||
            !string.Equals(previousName, name, StringComparison.OrdinalIgnoreCase)) return ValueTask.FromResult(false);
        return new ValueTask<bool>(Task.Run(() =>
        {
            OnProvingCaseSpelling(grant: true);
            var observed = GetFinalPath(file);
            return (admittedSpelling != null && admittedSpelling == observed || SameLocationOnWorker(targetPath, observed)) &&
                file.Path == path && file.Name == name;
        }));
    }

    static partial void OnProvingCaseSpelling(bool grant);

    private static SafeFileHandle OpenMetadataByName(string path, uint flags = 0)
    {
        bool unavailable = false;
        OnOpeningNamedMetadata(path, ref unavailable);
        if (unavailable)
        {
            Marshal.SetLastPInvokeError(5);
            return new SafeFileHandle(IntPtr.Zero, ownsHandle: false);
        }
        return CreateFileFromAppW(OpenableLocalVolumePath(path), 0, FileShare.ReadWrite | FileShare.Delete,
            IntPtr.Zero, FileMode.Open, flags, IntPtr.Zero);
    }

    static partial void OnOpeningNamedMetadata(string path, ref bool unavailable);

    // IStorageItemHandleAccess::Create requests FILE_READ_ATTRIBUTES on the user's grant.
    // The managed CreateSafeFileHandle overload would request file data, which can be
    // unavailable while the original file is pinned by another reader or by our transaction.
    private static SafeFileHandle OpenGrantedMetadata(StorageFile file)
    {
        try { return OpenGrantedAttributes(file); }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex) ||
            ex is NotSupportedException or NotImplementedException or InvalidCastException or ArgumentException)
        {
            // Providers that only implement data reads can still identify their grant.
            return file.CreateSafeFileHandle(FileAccess.Read, FileShare.ReadWrite | FileShare.Delete, FileOptions.None);
        }
    }

    private static unsafe SafeFileHandle OpenGrantedAttributes(StorageFile file)
    {
        using var access = ((IWinRTObject)file).NativeObject.As<IUnknownVftbl>(
            new Guid("5CA296B2-2C25-4D22-B785-B885C8201E6A"));
        var self = access.ThisPtr;
        var create = (delegate* unmanaged[Stdcall]<nint, uint, FileShare, uint, nint, nint*, int>)(*(nint**)self)[3];
        nint handle = 0;
        Marshal.ThrowExceptionForHR(create(self, 0x80 /* HAO_READ_ATTRIBUTES */, FileShare.ReadWrite | FileShare.Delete,
            0, 0, &handle));
        return new SafeFileHandle(handle, ownsHandle: true);
    }

    internal static async ValueTask RequireLocationAsync(string expected, string observed, DocumentSaveStage stage = DocumentSaveStage.Conflict)
    {
        if (expected != null && !await SameLocationAsync(expected, observed).ConfigureAwait(false))
            throw new DocumentSaveException(stage);
    }

    internal static bool RequiresLocationConfirmation(string expected, string observed) => expected == null && observed != null;

    // The provider's equality hint folds case even on case-sensitive filesystems.
    // Ambiguous spellings need the same filesystem proof used by save admission.
    private static bool NeedsFileCaseProof(StorageFile first, StorageFile second) =>
        !ReferenceEquals(first, second) && !string.IsNullOrEmpty(first.Path) &&
        !string.Equals(first.Path, second.Path, StringComparison.Ordinal) &&
        string.Equals(first.Path, second.Path, StringComparison.OrdinalIgnoreCase);

    internal static bool SameFileHint(StorageFile first, StorageFile second) =>
        first != null && second != null && first.IsEqual(second) && !NeedsFileCaseProof(first, second);

    internal static async ValueTask<bool> MatchesFileHintAsync(StorageFile first, StorageFile second,
        CancellationToken cancellation = default) =>
        first != null && second != null && first.IsEqual(second) &&
        (!NeedsFileCaseProof(first, second) || await IsSameFileAsync(first, second, cancellation).ConfigureAwait(false));

    private static async ValueTask RequireCurrentLocationAsync(Identity previous, CancellationToken cancellation)
    {
        var current = await ResolveAsync(previous.File, cancellation).ConfigureAwait(false);
        await RequireLocationAsync(previous.FinalPath, current.FinalPath).ConfigureAwait(false);
    }

    internal static async Task<bool> IsSameFileAsync(StorageFile first, StorageFile second, CancellationToken cancellation)
    {
        if (ReferenceEquals(first, second)) return true;
        if (first == null || second == null) return false;
        var left = await ResolveAsync(first, cancellation).ConfigureAwait(false);
        var right = await ResolveAsync(second, cancellation).ConfigureAwait(false);
        return await left.MatchesAsync(right).ConfigureAwait(false) == true;
    }

    internal static async Task<FileLockLease> AcquireAsync(StorageFile file, CancellationToken cancellation) =>
        await AcquireAsync(await ResolveAsync(file, cancellation).ConfigureAwait(false), cancellation).ConfigureAwait(false);

    internal static async Task<FileLockLease> AcquireAsync(Identity target, CancellationToken cancellation)
    {
        using var measurement = OperationMetrics.Measure("document.save.lease");
        try
        {
            var path = GetLockName(target.FinalPath) ?? target.File.Path;
            if (string.IsNullOrEmpty(path)) return null; // The provider transaction still guards the version check.
            var key = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(path)));
            var lockPath = Path.Combine(ApplicationData.Current.LocalFolder.Path, "DocumentSaveLocks", key + ".lock");
            return await FileLockLease.AcquireTemporaryExclusiveAsync(lockPath, TimeSpan.FromSeconds(5), cancellation).ConfigureAwait(false);
        }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex) || ex is TimeoutException)
        {
            throw new DocumentSaveException(DocumentSaveStage.Coordinate, ex);
        }
    }

    internal static string GetLockName(string finalPath)
    {
        if (finalPath == null) return null;
        // Keep one lease across case-only changes to any component. Distinct
        // case-sensitive entries may share a lease; identity/version proof stays separate.
        return finalPath.ToUpperInvariant();
    }

    private static string GetFinalPath(StorageFile file)
    {
        var path = file.Path;
        if (!string.IsNullOrEmpty(path))
        {
            // Query only metadata. This also works while a writer denies data access.
            var nativePath = path;
            if (Path.IsPathFullyQualified(path) && !path.StartsWith(@"\\?\", StringComparison.Ordinal) &&
                !path.StartsWith(@"\\.\", StringComparison.Ordinal))
                nativePath = path.StartsWith(@"\\", StringComparison.Ordinal) ? @"\\?\UNC\" + path[2..] : @"\\?\" + path;
            using var handle = OpenMetadataByName(nativePath);
            var final = GetFinalPath(handle);
            if (final != null) return final;
        }

        // Picker grants can be available only through the broker.
        try
        {
            using var granted = OpenGrantedMetadata(file);
            return GetFinalPath(granted);
        }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex) || ex is NotSupportedException or NotImplementedException or InvalidCastException)
        {
            return null;
        }
    }

    private static unsafe string GetFinalPath(SafeFileHandle handle)
    {
        if (handle == null || handle.IsInvalid) return null;
        // Prefer a volume GUID across restarts. Network redirects may only expose an NT name.
        bool hideVolumeGuid = false;
        OnResolvingVolumePath(ref hideVolumeGuid);
        return (hideVolumeGuid ? null : GetFinalPath(handle, 1)) ?? GetFinalPath(handle, 2);
    }

    static partial void OnResolvingVolumePath(ref bool hideVolumeGuid);

    private static unsafe string GetFinalPath(SafeFileHandle handle, uint volumeFormat)
    {
        if (handle == null || handle.IsInvalid) return null;
        Span<char> buffer = stackalloc char[512];
        while (true)
        {
            uint length;
            fixed (char* pointer = buffer)
                length = GetFinalPathNameByHandleW(handle, pointer, (uint)buffer.Length, volumeFormat);
            if (length == 0) return null;
            if (length < buffer.Length) return new string(buffer[..(int)length]);
            buffer = new char[checked((int)length)];
        }
    }

    [LibraryImport("api-ms-win-core-file-fromapp-l1-1-0.dll", StringMarshalling = StringMarshalling.Utf16, SetLastError = true)]
    private static partial SafeFileHandle CreateFileFromAppW(string path, uint access, FileShare share,
        IntPtr securityAttributes, FileMode disposition, uint flags, IntPtr template);

    [LibraryImport("api-ms-win-core-file-l1-2-1.dll")]
    private static unsafe partial uint GetFinalPathNameByHandleW(SafeFileHandle handle, char* buffer, uint capacity, uint flags);

    internal static async Task<DocumentFileVersion> ReadVersionAsync(StorageFile file,
        CancellationToken cancellationToken = default, long? expectedLength = null)
    {
        var stage = "resolve";
        try
        {
            var target = await ResolveAsync(file, cancellationToken).ConfigureAwait(false);
            IRandomAccessStream pinned;
            try
            {
                // Pin the very first observation, including size and timestamp.
                stage = "open";
                pinned = await FileStorage.OpenReadAsync(file, StorageOpenOptions.AllowOnlyReaders, cancellationToken);
            }
            catch (Exception ex) when (!expectedLength.HasValue && ex.HResult == unchecked((int)0x80070005))
            {
                // A picker may grant only write access to a new, empty destination.
                // Its exact bytes are known; the transaction must still verify them.
                stage = "empty-file metadata";
                var properties = await FileStorage.GetFilePropertiesAsync(file, cancellationToken);
                if (properties.Size == 0)
                {
                    await RequireCurrentLocationAsync(target, cancellationToken).ConfigureAwait(false);
                    return new DocumentFileVersion(properties.DateModified.ToFileTime(), EmptyFileSha256, target.FinalPath);
                }
                throw;
            }
            using var view = pinned;
            stage = "size";
            if (expectedLength.HasValue && view.Size != (ulong)expectedLength.Value)
                throw new DocumentSaveException(DocumentSaveStage.Superseded);
            Task observation = null;
            OnVersionRead(file, cancellationToken, ref observation);
            if (observation != null) await observation;
            stage = "location before read";
            await RequireCurrentLocationAsync(target, cancellationToken).ConfigureAwait(false);
            stage = "metadata before read";
            var time = (await FileStorage.GetFilePropertiesAsync(file, cancellationToken)).DateModified.ToFileTime();
            if (view.Size == 0) return new DocumentFileVersion(time, EmptyFileSha256, target.FinalPath);
            stage = "input stream";
            using var input = view.GetInputStreamAt(0);
            stage = "hash";
            var digest = await ReadSha256Async(input, cancellationToken).ConfigureAwait(false);
            stage = "metadata after read";
            if ((await FileStorage.GetFilePropertiesAsync(file, cancellationToken)).DateModified.ToFileTime() != time) throw new DocumentSaveException(DocumentSaveStage.Conflict);
            stage = "location after read";
            await RequireCurrentLocationAsync(target, cancellationToken).ConfigureAwait(false);
            stage = "close";
            return new DocumentFileVersion(time, digest, target.FinalPath);
        }
        catch (Exception ex) when (ex is not DocumentSaveException and not OperationCanceledException)
        {
            LoggingService.LogError($"[{nameof(DocumentSaveTarget)}] Version observation failed at {stage} (0x{ex.HResult:X8}): {ex}");
            if (DocumentSaveException.IsStorageError(ex)) throw new DocumentSaveException(DocumentSaveStage.VerifyVersion, ex);
            throw;
        }
    }

    static partial void OnVersionRead(StorageFile file, CancellationToken cancellation, ref Task observation);

    static partial void OnOriginalVersionRead(StorageFile file, CancellationToken cancellation, ref Task observation);

    internal static async Task VerifyAsync(StorageFile file, DocumentFileVersion expected, IInputStream original, CancellationToken cancellationToken)
    {
        string digest;
        try
        {
            // A sync client may touch the timestamp without changing any bytes.
            // The transaction pins the original, so its fingerprint decides conflicts.
            Task observation = null;
            OnOriginalVersionRead(file, cancellationToken, ref observation);
            if (observation != null) await observation;
            digest = await ReadSha256Async(original, cancellationToken).ConfigureAwait(false);
            if (string.Equals(digest, expected.Sha256, StringComparison.OrdinalIgnoreCase)) return;
        }
        catch (Exception ex) when (DocumentSaveException.IsStorageError(ex))
        {
            throw new DocumentSaveException(DocumentSaveStage.VerifyVersion, ex);
        }
        throw new DocumentSaveException(DocumentSaveStage.Conflict) { ObservedOriginalSha256 = digest };
    }

    private static async Task<string> ReadSha256Async(IInputStream input, CancellationToken cancellationToken)
    {
        // Cached WinRT reads can complete synchronously for the entire file.
        // Start the scan on the pool so a save from the UI keeps processing input.
        if (!Thread.CurrentThread.IsThreadPoolThread)
            await Task.CompletedTask.ConfigureAwait(ConfigureAwaitOptions.ForceYielding);
        cancellationToken.ThrowIfCancellationRequested();
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var buffer = ArrayPool<byte>.Shared.Rent(VerificationBufferSize);
        try
        {
            var chunk = buffer.AsBuffer(0, VerificationBufferSize);
            while (true)
            {
                cancellationToken.ThrowIfCancellationRequested();
                chunk.Length = 0;
                // AsTask(token) can finish before the native read releases this array.
                var read = await input.ReadAsync(chunk, VerificationBufferSize, InputStreamOptions.Partial)
                    .AsCompletionTask(cancellationToken).ConfigureAwait(false);
                cancellationToken.ThrowIfCancellationRequested();
                var count = checked((int)(read?.Length ?? 0));
                if (count == 0) return Convert.ToHexString(hash.GetHashAndReset());
                if (count > VerificationBufferSize) throw new InvalidDataException("A file read exceeded the requested buffer size.");
                if (!read.IsSameData(chunk)) read.CopyTo(0, buffer, 0, count);
                hash.AppendData(buffer, 0, count);
            }
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(buffer, clearArray: true);
        }
    }
}
