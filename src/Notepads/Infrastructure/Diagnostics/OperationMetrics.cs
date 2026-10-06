// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Diagnostics.Metrics;
using System.Threading;

namespace Notepads.Infrastructure.Diagnostics;

/// <summary>Optional observations that never participate in an operation's success or ownership.</summary>
internal static class OperationMetrics
{
    public const string MeterName = "Notepads.Operations";
    private static int _disabled;

    public static Measurement Measure(string operation)
    {
        if (Volatile.Read(ref _disabled) != 0) return default;
        try { return Instruments.Duration.Enabled ? new Measurement(operation, Stopwatch.GetTimestamp()) : default; }
        catch (Exception) { Volatile.Write(ref _disabled, 1); return default; }
    }

    public static void MetadataReadAttempt() => RecordCounter(CounterKind.Reads, 1);
    public static void MetadataBytesRead(long bytes) => RecordCounter(CounterKind.ReadBytes, bytes);
    public static void MetadataCopyWritten(long bytes)
    {
        RecordCounter(CounterKind.Writes, 1);
        RecordCounter(CounterKind.WriteBytes, bytes);
    }

    private enum CounterKind { Reads, ReadBytes, Writes, WriteBytes }

    private static void RecordCounter(CounterKind instrument, long value)
    {
        if (Volatile.Read(ref _disabled) != 0) return;
        try
        {
            var counter = instrument switch
            {
                CounterKind.Reads => Instruments.Reads,
                CounterKind.ReadBytes => Instruments.ReadBytes,
                CounterKind.Writes => Instruments.Writes,
                _ => Instruments.WriteBytes
            };
            if (counter.Enabled) counter.Add(value);
        }
        catch (Exception) { Volatile.Write(ref _disabled, 1); }
    }

    public readonly struct Measurement : IDisposable
    {
        private readonly string _operation;
        private readonly long _started;

        internal Measurement(string operation, long started) { _operation = operation; _started = started; }

        public void Dispose()
        {
            if (_operation == null || Volatile.Read(ref _disabled) != 0) return;
            try
            {
                Instruments.Duration.Record(Stopwatch.GetElapsedTime(_started).TotalMilliseconds,
                    new KeyValuePair<string, object>("operation", _operation));
            }
            catch (Exception) { Volatile.Write(ref _disabled, 1); }
        }
    }

    // A listener can throw while an instrument is published as well as while
    // it receives a measurement. Keep initialization inside the guarded calls.
    private static class Instruments
    {
        static Instruments() { }
        private static readonly Meter Meter = new(MeterName, "2.0.0");
        internal static readonly Histogram<double> Duration = Meter.CreateHistogram<double>("notepads.operation.duration", "ms");
        internal static readonly Counter<long> Reads = Meter.CreateCounter<long>("notepads.session.metadata.reads");
        internal static readonly Counter<long> ReadBytes = Meter.CreateCounter<long>("notepads.session.metadata.read.bytes", "By");
        internal static readonly Counter<long> Writes = Meter.CreateCounter<long>("notepads.session.metadata.writes");
        internal static readonly Counter<long> WriteBytes = Meter.CreateCounter<long>("notepads.session.metadata.write.bytes", "By");
    }
}
