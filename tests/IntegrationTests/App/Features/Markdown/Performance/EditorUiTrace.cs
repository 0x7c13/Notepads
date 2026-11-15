// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text.Json;

namespace NotepadsIntegrationTests;

// This bridge is compiled only into the isolated App test host. Its referenced
// editor variant exports bounded numeric diagnostics; product DLLs do not.
internal sealed partial class EditorUiTrace
{
    private const uint AbiVersion = 1;
    private const int CapacityWords = 8192 * 6;
    private static readonly ulong[] Buffer = new ulong[CapacityWords];
    private readonly ulong _generation;
    private bool _stopped;

    private EditorUiTrace(ulong generation) => _generation = generation;

    internal static void Prepare() => GC.KeepAlive(Buffer);

    internal static EditorUiTrace Start()
    {
        Check(StartNative(AbiVersion, out var generation, out var frequency));
        if (generation == 0 || frequency != checked((ulong)Stopwatch.Frequency))
        {
            unsafe { fixed (ulong* words = Buffer) StopNative(AbiVersion, generation, words, CapacityWords, out _, out _); }
            throw new InvalidOperationException("Editor trace clock or generation mismatch.");
        }
        return new EditorUiTrace(generation);
    }

    internal unsafe Result Stop()
    {
        if (_stopped) throw new InvalidOperationException("The editor trace was already stopped.");
        uint written;
        ulong dropped;
        fixed (ulong* words = Buffer)
            Check(StopNative(AbiVersion, _generation, words, CapacityWords, out written, out dropped));
        _stopped = true;
        if (written > CapacityWords || written % 6 != 0)
            throw new InvalidOperationException("The editor trace returned an invalid record length.");
        return new Result(_generation, dropped, Buffer.AsSpan(0, checked((int)written)).ToArray());
    }

    private static void Check(uint error)
    {
        if (error != 0) throw new Win32Exception(checked((int)error), "The isolated editor UI trace failed.");
    }

    internal sealed record Result(ulong Generation, ulong Dropped, ulong[] Words)
    {
        internal void Write(Utf8JsonWriter json)
        {
            json.WriteStartObject("editorTrace");
            json.WriteNumber("abi", AbiVersion); json.WriteNumber("generation", Generation);
            json.WriteNumber("droppedScopes", Dropped);
            json.WriteString("scope", "Completed source-editor UI scopes on the same QPC clock; synchronous wall time, not sampled CPU or GPU presents.");
            json.WriteStartArray("records");
            for (var index = 0; index < Words.Length; index += 6)
            {
                var name = Words[index] switch { 1 => "editor-idle", 2 => "editor-updates", _ => throw new InvalidOperationException("Unknown editor trace phase.") };
                json.WriteStartObject(); json.WriteString("phase", name);
                json.WriteNumber("id", Words[index + 1]); json.WriteNumber("parent", Words[index + 2]);
                json.WriteNumber("threadId", Words[index + 3]); json.WriteNumber("startQpc", Words[index + 4]);
                json.WriteNumber("endQpc", Words[index + 5]); json.WriteEndObject();
            }
            json.WriteEndArray(); json.WriteEndObject();
        }
    }

    [LibraryImport("WinUIEditor.dll", EntryPoint = "NotepadsEditorUiTraceStart")]
    [UnmanagedCallConv(CallConvs = new[] { typeof(CallConvCdecl) })]
    private static partial uint StartNative(uint abiVersion, out ulong generation, out ulong frequency);

    [LibraryImport("WinUIEditor.dll", EntryPoint = "NotepadsEditorUiTraceStop")]
    [UnmanagedCallConv(CallConvs = new[] { typeof(CallConvCdecl) })]
    private static unsafe partial uint StopNative(uint abiVersion, ulong generation, ulong* words, uint capacityWords,
        out uint writtenWords, out ulong droppedScopes);
}
