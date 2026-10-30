// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Security.Cryptography;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Sessions.Contracts;
using Notepads.Features.Sessions.Transfer;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

// Owns the native read lease for the complete reserved move. The workspace sees
// only its managed request/export contract, never a native control or reader.
internal sealed partial class WindowMoveCapture : IDisposable
{
    private readonly EditorStateReader _reader;
    internal WindowMoveRequest Request { get; }

    internal WindowMoveCapture(Editor native, WindowMoveRequest request)
    {
        Request = request;
        _reader = native.AcquireStateReader();
    }

    internal Task ExportAsync(CancellationToken cancellation) => Task.Run(async () =>
    {
        Request.StateBytes = checked((long)_reader.Length);
        if (Request.StateBytes > WindowMoveRequest.MaximumStateBytes) throw new InvalidDataException("Live editing state is too large to move.");
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        await using var output = WindowMoveStore.CreateState(Request.Source);
        long written = 0;
        while (written < Request.StateBytes)
        {
            cancellation.ThrowIfCancellationRequested();
            var bytes = (await _reader.ReadAsync()).ToArray();
            if (bytes.Length == 0 || bytes.Length > Request.StateBytes - written)
                throw new InvalidDataException("Live editing state changed while reading.");
            hash.AppendData(bytes);
            await output.WriteAsync(bytes, cancellation);
            written += bytes.Length;
        }
        output.Flush(flushToDisk: true);
        Request.StateSha256 = Convert.ToHexStringLower(hash.GetHashAndReset());
    }, cancellation);

    public void Dispose() => _reader.Dispose();
}
