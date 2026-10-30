// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Diagnostics;
using System.Globalization;
using NotepadsTestIsolation;

namespace Notepads.UnitTests;

public sealed class TestProcessLifetimeTests : IDisposable
{
    private readonly string _root = Directory.CreateTempSubdirectory("Notepads-Retention-").FullName;
    private readonly string _marker;
    private readonly string _created;

    public TestProcessLifetimeTests()
    {
        using var process = Process.GetCurrentProcess();
        _marker = Path.Combine(_root, $"retained-process-{process.Id}.txt");
        _created = process.StartTime.ToUniversalTime().Ticks.ToString(CultureInfo.InvariantCulture);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("0")]
    [InlineData("not a creation time")]
    public async Task MissingOrStaleAcknowledgementKeepsTheHostAlive(string initial)
    {
        if (initial != null) File.WriteAllText(_marker, initial);
        var retention = TestProcessLifetime.WaitForRetentionAsync(_root);
        try { Assert.False(retention.IsCompleted); }
        finally
        {
            File.WriteAllText(_marker, _created);
            await retention.WaitAsync(TimeSpan.FromSeconds(10));
        }
    }

    [Fact]
    public async Task AlreadyRetainedProcessCanProceed()
    {
        File.WriteAllText(_marker, _created);
        await TestProcessLifetime.WaitForRetentionAsync(_root).WaitAsync(TimeSpan.FromSeconds(10));
    }

    public void Dispose() => Directory.Delete(_root, recursive: true);
}
