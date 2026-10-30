// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

// Single-threaded script helper. English counter lookup is independent of the
// Windows display language and avoids the formatted-data WMI provider.
public sealed class ProcessorUtilitySampler : IDisposable
{
    private readonly QueryHandle query;
    private readonly IntPtr counter;

    public ProcessorUtilitySampler()
    {
        Check(PdhOpenQueryW(null, UIntPtr.Zero, out query), "open");
        try
        {
            Check(PdhAddEnglishCounterW(query, @"\Processor Information(_Total)\% Processor Utility",
                UIntPtr.Zero, out counter), "add processor utility");
            Check(PdhCollectQueryData(query), "prime");
        }
        catch { query.Dispose(); throw; }
    }

    // Caller waits between samples. Rate counters need the priming collection
    // plus a later collection; do not cap turbo/multicore values at 100 percent.
    public double Read()
    {
        if (query.IsClosed) throw new ObjectDisposedException(nameof(ProcessorUtilitySampler));
        Check(PdhCollectQueryData(query), "collect");
        const uint DoubleNoCap100 = 0x00000200 | 0x00008000;
        Check(PdhGetFormattedCounterValue(counter, DoubleNoCap100, IntPtr.Zero, out var value), "format");
        if (value.Status > 1) Check(value.Status, "counter status"); // VALID_DATA / NEW_DATA
        return value.Value;
    }

    public void Dispose() => query.Dispose();

    private static void Check(uint status, string operation)
    {
        if (status != 0) throw new InvalidOperationException($"PDH {operation} failed: 0x{status:X8}.");
    }

    [StructLayout(LayoutKind.Explicit)]
    private struct CounterValue
    {
        [FieldOffset(0)] public uint Status;
        [FieldOffset(8)] public double Value;
    }

    private sealed class QueryHandle : SafeHandleZeroOrMinusOneIsInvalid
    {
        public QueryHandle() : base(true) { }
        protected override bool ReleaseHandle() => PdhCloseQuery(handle) == 0;
    }

    [DllImport("pdh.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern uint PdhOpenQueryW(string source, UIntPtr data, out QueryHandle query);
    [DllImport("pdh.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern uint PdhAddEnglishCounterW(QueryHandle query, string path, UIntPtr data, out IntPtr counter);
    [DllImport("pdh.dll", ExactSpelling = true)]
    private static extern uint PdhCollectQueryData(QueryHandle query);
    [DllImport("pdh.dll", ExactSpelling = true)]
    private static extern uint PdhGetFormattedCounterValue(IntPtr counter, uint format, IntPtr type, out CounterValue value);
    [DllImport("pdh.dll", ExactSpelling = true)]
    private static extern uint PdhCloseQuery(IntPtr query);
}
