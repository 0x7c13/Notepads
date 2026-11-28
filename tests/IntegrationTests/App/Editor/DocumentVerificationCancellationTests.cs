// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using Notepads.Features.Documents.Storage;
using Notepads.Infrastructure.Threading;
using Windows.Foundation;
using Windows.Storage.Streams;

namespace NotepadsEditorTests;

internal static partial class DocumentVerificationCancellationTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var entered = Signal();
        var release = Signal();
        var canceled = Signal();
        var drained = Signal();
        var suppressLateWrite = false;
        using var cancellation = new CancellationTokenSource();
        using var input = new ControlledInput(async (buffer, count, token) =>
        {
            using var registration = token.Register(() => canceled.TrySetResult());
            entered.TrySetResult();
            try
            {
                await release.Task;
                // A failing old implementation may already have returned its array
                // to the pool. Do not corrupt an unrelated renter during the red run.
                if (!suppressLateWrite) new byte[] { 0x5a }.CopyTo(0, buffer, 0, 1);
                buffer.Length = suppressLateWrite ? 0u : 1u;
                return buffer;
            }
            finally { drained.TrySetResult(); }
        });
        var hashing = DocumentSaveTarget.TestReadHashAsync(input, cancellation.Token);
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
            cancellation.Cancel();
            await Task.WhenAny(hashing, Task.Delay(250));
            suppressLateWrite = hashing.IsCompleted;
            Check(!hashing.IsCompleted && !input.Closed,
                "Canceling verification must not release its pooled buffer or stream before the pending WinRT read ends.");
            await canceled.Task.WaitAsync(TimeSpan.FromSeconds(5));
        }
        finally
        {
            release.TrySetResult();
            try { await hashing.WaitAsync(TimeSpan.FromSeconds(5)); }
            catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
            await drained.Task.WaitAsync(TimeSpan.FromSeconds(5));
        }

        const int chunk = 256 * 1024;
        var bytes = new byte[2 * chunk + 17];
        new Random(17).NextBytes(bytes);
        foreach (var sameBuffer in new[] { false, true })
        {
            var offset = 0;
            IBuffer supplied = null;
            using var shortReads = new ControlledInput((buffer, count, _) =>
            {
                Check(count == chunk && buffer.Capacity == chunk && buffer.Length == 0 &&
                    (supplied == null || supplied.IsSameData(buffer)),
                    "Every verification read must reset and reuse one 256 KiB buffer.");
                supplied ??= buffer;
                var size = Math.Min(8191, bytes.Length - offset);
                IBuffer result;
                if (sameBuffer)
                {
                    bytes.CopyTo(offset, buffer, 0, size);
                    buffer.Length = (uint)size;
                    result = buffer;
                }
                else result = bytes.AsBuffer(offset, size);
                offset += size;
                return Task.FromResult(result);
            });
            var digest = await DocumentSaveTarget.TestReadHashAsync(shortReads, default);
            Check(digest == Convert.ToHexString(SHA256.HashData(bytes)) && !shortReads.Closed,
                "Short reads into the same or a provider-returned buffer must hash exact bytes without closing a borrowed stream.");
        }
        for (var mode = 0; mode < 3; mode++) await CodecReadCancellationAsync(mode);
        await LeasedCopyCancellationAsync();
        for (var mode = 0; mode < 3; mode++) await CodecWriteCancellationAsync(mode);
        await CancellationCallbackFailureAsync();
        log.AppendLine("PASS: canceled verification drains its pending native read before buffer release; short/different returned buffers preserve exact SHA-256 with one 256 KiB input buffer.");
        log.AppendLine("PASS: canceled decode, encode, line-count and canonical-baseline writes retain pooled I/O buffers until the current native chunk completes.");
    }

    private static async Task CodecReadCancellationAsync(int mode)
    {
        var entered = Signal();
        var release = Signal();
        var skipWrite = false;
        using var cancellation = new CancellationTokenSource();
        using var native = new ControlledInput(async (buffer, _, _) =>
        {
            entered.TrySetResult();
            await release.Task;
            if (!skipWrite) new byte[] { (byte)'A' }.CopyTo(0, buffer, 0, 1);
            buffer.Length = skipWrite ? 0u : 1u;
            return buffer;
        });
        using var input = native.AsStreamForRead();
        using var output = new MemoryStream();
        Task work = mode switch
        {
            0 => DocumentTextCodec.DecodeToCanonicalUtf8Async(input, output, new UTF8Encoding(false), cancellationToken: cancellation.Token),
            1 => CanonicalLineCounter.CountAsync(input, cancellation.Token),
            _ => DocumentTextCodec.EncodeAsync(input, output, new UTF8Encoding(false), LineEnding.Cr, cancellation.Token)
        };
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
            cancellation.Cancel();
            await Task.WhenAny(work, Task.Delay(100));
            skipWrite = work.IsCompleted;
            Check(!work.IsCompleted, $"Canceled codec read mode={mode} must drain native I/O before returning pooled memory.");
        }
        finally
        {
            release.TrySetResult();
            try { await work.WaitAsync(TimeSpan.FromSeconds(5)); }
            catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
        }
    }

    private static async Task LeasedCopyCancellationAsync()
    {
        var entered = Signal();
        var release = Signal();
        var skipWrite = false;
        using var cancellation = new CancellationTokenSource();
        using var native = new ControlledInput(async (buffer, _, _) =>
        {
            entered.TrySetResult();
            await release.Task;
            if (!skipWrite) new byte[] { (byte)'A' }.CopyTo(0, buffer, 0, 1);
            buffer.Length = skipWrite ? 0u : 1u;
            return buffer;
        });
        using var input = native.AsStreamForRead();
        using var output = new MemoryStream();
        using var asset = DocumentAssetLease.Acquire(null);
        async Task CopyAndCloseAsync()
        {
            using var leased = asset.TestWrapReadStream(input, 65536);
            await leased.CopyToAsync(output, 65536, cancellation.Token);
        }
        var work = CopyAndCloseAsync();
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
            cancellation.Cancel();
            await Task.WhenAny(work, Task.Delay(100));
            skipWrite = work.IsCompleted;
            Check(!work.IsCompleted && asset.TestLeaseCount == 2 && !native.Closed,
                "Canceled baseline CopyToAsync must retain its pooled buffer, source and lease until native I/O ends.");
        }
        finally
        {
            release.TrySetResult();
            try { await work.WaitAsync(TimeSpan.FromSeconds(5)); }
            catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { }
        }
        Check(asset.TestLeaseCount == 1 && native.Closed,
            "Draining a canceled baseline copy must close its reader and release exactly its own asset lease.");
    }

    private static async Task CodecWriteCancellationAsync(int mode)
    {
        var entered = Signal();
        var release = Signal();
        using var cancellation = new CancellationTokenSource();
        using var native = new ControlledOutput(async buffer =>
        {
            entered.TrySetResult();
            await release.Task;
            return buffer.Length;
        });
        using var output = native.AsStreamForWrite();
        var bytes = Encoding.UTF8.GetBytes(new string('A', 65536));
        using var input = new MemoryStream(bytes);
        using var writer = mode == 0 ? null : DocumentBaseline.TestCreateCanonicalWriter(output,
            mode == 1 ? cancellation.Token : CancellationToken.None);
        Task work = mode != 0 ? writer.WriteAsync(bytes, 0, bytes.Length,
            mode == 2 ? cancellation.Token : CancellationToken.None) :
            DocumentTextCodec.EncodeAsync(input, output, new UTF8Encoding(false), LineEnding.Cr, cancellation.Token);
        try
        {
            await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
            cancellation.Cancel();
            await Task.WhenAny(work, Task.Delay(100));
            Check(!work.IsCompleted, $"Canceled codec/baseline write mode={mode} must drain native I/O before returning pooled memory.");
        }
        finally
        {
            release.TrySetResult();
            var observedCancellation = false;
            try { await work.WaitAsync(TimeSpan.FromSeconds(5)); }
            catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { observedCancellation = true; }
            Check(observedCancellation, "The drained codec/baseline write must still report the requested cancellation.");
        }
    }

    private static async Task CancellationCallbackFailureAsync()
    {
        // These are the same transport errors ignored by SDK 87's own
        // AsyncInfoToTaskBridge cancellation callback (not arbitrary failures).
        foreach (var error in new[] { unchecked((int)0x80010108), unchecked((int)0x800706BA), unchecked((int)0x89020001) })
        {
            using (var canceled = new CancellationTokenSource())
            {
                canceled.Cancel();
                var complete = new CancelFailureOperation(AsyncInfo.Run<IBuffer, uint>((_, _) =>
                    Task.FromResult(new byte[0].AsBuffer())), error);
                try
                {
                    var result = await complete.AsCompletionTask(canceled.Token);
                    Check(result.Length == 0 && complete.CancelCalls == 0,
                        "A completed native operation must not receive a late cancellation callback.");
                }
                finally { complete.Close(); }
            }

            var entered = Signal();
            var release = Signal();
            using var cancellation = new CancellationTokenSource();
            CancelFailureOperation operation = null;
            using var input = new OperationInput(() => operation = new CancelFailureOperation(
                AsyncInfo.Run<IBuffer, uint>(async (_, _) =>
                {
                    entered.TrySetResult();
                    await release.Task;
                    return new byte[] { 0x5a }.AsBuffer();
                }), error));
            var hashing = DocumentSaveTarget.TestReadHashAsync(input, cancellation.Token);
            try
            {
                await entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
                cancellation.Cancel();
                Check(operation.CancelCalls == 1 && !hashing.IsCompleted,
                    "A failed native cancellation request must not escape Cancel or skip the native completion wait.");
            }
            finally
            {
                release.TrySetResult();
                var observedCancellation = false;
                try { await hashing.WaitAsync(TimeSpan.FromSeconds(5)); }
                catch (OperationCanceledException) when (cancellation.IsCancellationRequested) { observedCancellation = true; }
                operation?.Close();
                Check(observedCancellation, "After draining a failed cancel request, the hash must honor its caller token.");
            }
        }
    }

    private static TaskCompletionSource Signal() => new(TaskCreationOptions.RunContinuationsAsynchronously);
    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class ControlledInput(Func<IBuffer, uint, CancellationToken, Task<IBuffer>> read) : IInputStream
    {
        internal bool Closed;
        public IAsyncOperationWithProgress<IBuffer, uint> ReadAsync(IBuffer buffer, uint count, InputStreamOptions options) =>
            AsyncInfo.Run<IBuffer, uint>((token, _) => read(buffer, count, token));
        public void Dispose() => Closed = true;
    }

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class ControlledOutput(Func<IBuffer, Task<uint>> write) : IOutputStream
    {
        public IAsyncOperationWithProgress<uint, uint> WriteAsync(IBuffer buffer) =>
            AsyncInfo.Run<uint, uint>((_, _) => write(buffer));
        public IAsyncOperation<bool> FlushAsync() => AsyncInfo.Run(_ => Task.FromResult(true));
        public void Dispose() { }
    }

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class OperationInput(Func<IAsyncOperationWithProgress<IBuffer, uint>> read) : IInputStream
    {
        public IAsyncOperationWithProgress<IBuffer, uint> ReadAsync(IBuffer buffer, uint count, InputStreamOptions options) => read();
        public void Dispose() { }
    }

    [WinRT.GeneratedWinRTExposedType]
    private sealed partial class CancelFailureOperation(IAsyncOperationWithProgress<IBuffer, uint> inner, int error)
        : IAsyncOperationWithProgress<IBuffer, uint>
    {
        private AsyncOperationWithProgressCompletedHandler<IBuffer, uint> _completed;
        internal int CancelCalls;
        public uint Id => inner.Id;
        public AsyncStatus Status => inner.Status;
        public Exception ErrorCode => inner.ErrorCode;
        public IBuffer GetResults() => inner.GetResults();
        public void Close() => inner.Close();
        public void Cancel() { CancelCalls++; throw new COMException("The native cancel transport disconnected.", error); }
        public AsyncOperationProgressHandler<IBuffer, uint> Progress { get => inner.Progress; set => inner.Progress = value; }
        public AsyncOperationWithProgressCompletedHandler<IBuffer, uint> Completed
        {
            get => _completed;
            set { _completed = value; inner.Completed = (_, status) => value(this, status); }
        }
    }
}
