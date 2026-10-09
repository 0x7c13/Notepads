// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Linq;
using System.Reflection.Metadata;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;

namespace NotepadsDragDropPeer;

// Control.DoDragDrop wraps this managed IDataObject in DataObject. The WinForms COM
// adapter delegates text requests here and owns CF_UNICODETEXT's terminated HGLOBAL.
// Text is never placed in the backing DataObject: doing so could bypass the gate.
internal sealed class GatedTextDataObject : ITypedDataObject
{
    private readonly PeerChannel channel;
    private readonly string caseId;
    private readonly string text;
    private readonly string mode;
    private readonly DataObject metadata = new();
    private int requestSequence;

    internal GatedTextDataObject(PeerChannel channel, string caseId, string text, string mode)
    {
        this.channel = channel;
        this.caseId = caseId;
        this.text = text;
        this.mode = mode;
    }

    public object? GetData(string format, bool autoConvert)
    {
        int requestId = Interlocked.Increment(ref requestSequence);
        channel.Event(caseId, "data-request-enter", requestId, format);
        if (format != DataFormats.UnicodeText)
        {
            // OLE/WinForms can also ask for drag-image metadata. Never gate those requests,
            // or advertise ANSI/serialized string fallback that could mask Unicode failure.
#pragma warning disable WFDEV005 // Metadata only; payload retrieval is the explicit Unicode path below.
            object? value = metadata.GetData(format, false);
#pragma warning restore WFDEV005
            channel.Event(caseId, "data-request-result", requestId, value is null ? "unsupported" : "metadata");
            return value;
        }

        string request = "ticks=" + DateTime.UtcNow.Ticks.ToString(CultureInfo.InvariantCulture) +
            "\r\npid=" + Environment.ProcessId.ToString(CultureInfo.InvariantCulture) +
            "\r\nthreadId=" + Environment.CurrentManagedThreadId.ToString(CultureInfo.InvariantCulture) +
            "\r\nrequestId=" + requestId.ToString(CultureInfo.InvariantCulture) +
            "\r\nformat=" + format + "\r\nmode=" + mode + "\r\n";
        channel.WriteAtomic(caseId + ".data-requested", request);
        channel.WriteAtomic(caseId + ".data-request-" + requestId.ToString("D4", CultureInfo.InvariantCulture), request);
        bool released = mode == "normal" || channel.WaitForRelease(caseId, "release-data", requestId);
        if (!released || mode == "fail")
        {
            channel.Event(caseId, "data-request-result", requestId, "failure");
            throw new COMException("Injected or timed-out Unicode data request.", unchecked((int)0x80004005));
        }

        channel.Event(caseId, "data-request-result", requestId, "success");
        return text;
    }

    public object? GetData(string format) => GetData(format, true);
    public object? GetData(Type format) => GetData(format == typeof(string) ? DataFormats.UnicodeText : format.FullName!, true);

    public bool TryGetData<[DynamicallyAccessedMembers(DynamicallyAccessedMemberTypes.All)] T>(
        [NotNullWhen(true), MaybeNullWhen(false)] out T data) =>
        TryGetData(typeof(T) == typeof(string) ? DataFormats.UnicodeText : typeof(T).FullName!, false, out data);

    public bool TryGetData<[DynamicallyAccessedMembers(DynamicallyAccessedMemberTypes.All)] T>(
        string format, [NotNullWhen(true), MaybeNullWhen(false)] out T data) =>
        TryGetData(format, false, out data);

    public bool TryGetData<[DynamicallyAccessedMembers(DynamicallyAccessedMemberTypes.All)] T>(
        string format, bool autoConvert, [NotNullWhen(true), MaybeNullWhen(false)] out T data)
    {
        // Typed requests use the same recorded gate. Only the immutable string and our
        // local drag metadata exist here; no serialized object payload is accepted.
        if (GetData(format, false) is T value)
        {
            data = value;
            return true;
        }

        data = default;
        return false;
    }

    public bool TryGetData<[DynamicallyAccessedMembers(DynamicallyAccessedMemberTypes.All)] T>(
        string format, Func<TypeName, Type?> resolver, bool autoConvert,
        [NotNullWhen(true), MaybeNullWhen(false)] out T data) =>
        TryGetData(format, false, out data);

    public bool GetDataPresent(string format, bool autoConvert) =>
        format == DataFormats.UnicodeText || metadata.GetDataPresent(format, false);

    public bool GetDataPresent(string format) => GetDataPresent(format, true);
    public bool GetDataPresent(Type format) => GetDataPresent(format == typeof(string) ? DataFormats.UnicodeText : format.FullName!, true);
    public string[] GetFormats(bool autoConvert) => new[] { DataFormats.UnicodeText }.Concat(metadata.GetFormats(false)).Distinct().ToArray();
    public string[] GetFormats() => GetFormats(true);

    public void SetData(string format, bool autoConvert, object? data)
    {
        if (format == DataFormats.UnicodeText || format == DataFormats.Text || format == DataFormats.OemText || format == DataFormats.StringFormat)
        {
            throw new InvalidOperationException("The drag payload is immutable.");
        }

        metadata.SetData(format, false, data);
    }

    public void SetData(string format, object? data) => SetData(format, false, data);
    public void SetData(Type format, object? data) => SetData(format.FullName!, false, data);
    public void SetData(object? data) => throw new NotSupportedException("An explicit metadata format is required.");
}
