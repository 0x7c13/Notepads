// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace NotepadsIntegrationTests;

internal sealed class TestReport
{
    public string Phase { get; set; }
    public string Status { get; set; }
    public long ElapsedMilliseconds { get; set; }
    internal static void Write(string phase, bool passed, long elapsed) => File.WriteAllText(
        Path.Combine(ApplicationData.Current.LocalFolder.Path, "report.json"),
        JsonSerializer.Serialize(new TestReport { Phase = phase, Status = passed ? "PASS" : "FAIL", ElapsedMilliseconds = elapsed },
            TestReportJsonContext.Default.TestReport));
}

[JsonSerializable(typeof(TestReport))]
[JsonSerializable(typeof(Dictionary<string, string>))]
internal partial class TestReportJsonContext : JsonSerializerContext;
