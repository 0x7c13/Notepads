// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text.Json;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.UnitTests;

public sealed class WindowMoveRequestTests
{
    private static WindowMoveRequest Request() => new()
    {
        Source = new TransferToken
        {
            TransferId = Guid.NewGuid(), SourceInstanceId = Guid.NewGuid(), SourceEditorId = Guid.NewGuid(),
            SourceOwnerId = Guid.NewGuid(), SourceEpochId = Guid.NewGuid(), Nonce = Guid.NewGuid(),
            DescriptorSha256 = new string('a', 64)
        },
        StateBytes = 264, StateSha256 = new string('b', 64), DetectedIndentation = -1,
        HasAddedLogEntry = true, IndentationChecked = true
    };

    [Fact]
    public void JsonRoundTripBindsLiveStateToTheExactSource()
    {
        var request = Request();
        var json = JsonSerializer.Serialize(request, SessionJsonContext.Default.WindowMoveRequest);
        var restored = JsonSerializer.Deserialize(json, SessionJsonContext.Default.WindowMoveRequest);
        restored.Validate();
        Assert.True(request.Matches(restored));
        restored.Source.Nonce = Guid.NewGuid();
        Assert.False(request.Matches(restored));
    }

    [Theory]
    [InlineData(0)] [InlineData(263)] [InlineData(268435457)] [InlineData(long.MaxValue)]
    public void ImpossibleStateSizesAreRejected(long bytes)
    {
        var request = Request(); request.StateBytes = bytes;
        Assert.Throws<InvalidDataException>(request.Validate);
    }

    [Theory]
    [InlineData("version")] [InlineData("hash")] [InlineData("sourceHash")]
    [InlineData("epoch")] [InlineData("missingSource")] [InlineData("indent")]
    public void InvalidRequestsNeverReachImport(string field)
    {
        var request = Request();
        switch (field)
        {
            case "version": request.Version = 2; break;
            case "hash": request.StateSha256 = new string('x', 64); break;
            case "sourceHash": request.Source.DescriptorSha256 = "short"; break;
            case "epoch": request.Source.SourceEpochId = Guid.Empty; break;
            case "missingSource": request.Source = null; break;
            case "indent": request.DetectedIndentation = 257; break;
        }
        Assert.Throws<InvalidDataException>(request.Validate);
    }
}
