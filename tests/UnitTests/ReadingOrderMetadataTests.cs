// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Text.Json;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.UnitTests;

public sealed class ReadingOrderMetadataTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void CapturedDirectionRoundtripsWithoutFollowingLaterChanges(bool rightToLeft)
    {
        var live = new DocumentMetadata { RightToLeftReadingOrder = rightToLeft };
        var captured = live.Clone();
        live.RightToLeftReadingOrder = !rightToLeft;
        var descriptor = new TextEditorSessionDataV2 { StateMetaData = captured };
        var json = JsonSerializer.Serialize(descriptor, SessionJsonContext.Default.TextEditorSessionDataV2);
        var restored = JsonSerializer.Deserialize(json, SessionJsonContext.Default.TextEditorSessionDataV2);
        Assert.Equal(rightToLeft, restored.StateMetaData.RightToLeftReadingOrder);
    }

    [Fact]
    public void DirectionChangeIsIncludedInRecoveryDescriptorIdentity()
    {
        var descriptor = new TextEditorSessionDataV2 { StateMetaData = new DocumentMetadata() };
        var ltr = descriptor.ComputeSha256();
        descriptor.StateMetaData.RightToLeftReadingOrder = true;
        Assert.NotEqual(ltr, descriptor.ComputeSha256());
        descriptor.StateMetaData.RightToLeftReadingOrder = false;
        Assert.Equal(ltr, descriptor.ComputeSha256());
    }

    [Fact]
    public void V1WithoutDirectionRestoresAsLtr()
    {
        const string json = """
            {"TextEditors":[{"Id":"00112233-4455-6677-8899-aabbccddeeff",
            "StateMetaData":{"FontZoomFactor":1,"WrapWord":true}}]}
            """;
        var legacy = JsonSerializer.Deserialize(json, SessionJsonContext.Default.NotepadsSessionDataV1);
        Assert.False(Assert.Single(legacy.TextEditors).StateMetaData.RightToLeftReadingOrder);
    }
}
