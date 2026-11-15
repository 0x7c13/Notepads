// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Controls.Markdown;

namespace Notepads.UnitTests;

public sealed class MarkdownDataImageTests
{
    private const string Png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP4z8DwHwyBNBgAAEnICff5q7YNAAAAAElFTkSuQmCC";

    [Theory]
    [InlineData("data:image/png;base64,")]
    [InlineData("DATA:IMAGE/PNG;BASE64,")]
    public void Valid_png_preserves_exact_bytes(string prefix)
    {
        byte[] expected = Convert.FromBase64String(Png);
        Assert.Equal(expected, MarkdownDataImage.Decode(prefix + Png, (ulong)expected.Length, TestContext.Current.CancellationToken));
        Assert.Null(MarkdownDataImage.Decode(prefix + Png, (ulong)expected.Length - 1, TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("data:image/png;base64,")]
    [InlineData("data:image/png;base64,!!!!")]
    [InlineData("data:image/png;base64,a===")]
    [InlineData("data:image/png;base64,a")]
    [InlineData("data:image/png;base64,AAAAAAAAAAAA")]
    [InlineData("data:image/png;base64,iVBORw0KGgo=\n")]
    [InlineData("data:image/png;base64,iVBORw0KGgo%3D")]
    [InlineData("data:image/png,%89PNG")]
    [InlineData("data:image/png;charset=utf-8;base64,iVBORw0KGgo=")]
    [InlineData("data:image/gif;base64,R0lGODlh")]
    [InlineData("data:image/svg+xml;base64,PHN2Zz4=")]
    [InlineData("https://example.com/picture.png")]
    public void Invalid_or_unsupported_payload_is_rejected(string destination) =>
        Assert.Null(MarkdownDataImage.Decode(destination, 1024, TestContext.Current.CancellationToken));

    [Theory]
    [InlineData(8)]
    [InlineData(9)]
    [InlineData(10)]
    [InlineData(24_575)]
    [InlineData(24_576)]
    [InlineData(24_577)]
    public void Padding_and_chunk_boundaries_preserve_bytes(int length)
    {
        byte[] bytes = new byte[length];
        new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 }.CopyTo(bytes, 0);
        for (int index = 8; index < length; ++index) bytes[index] = (byte)index;
        Assert.Equal(bytes, MarkdownDataImage.Decode("data:image/png;base64," + Convert.ToBase64String(bytes), (ulong)length, TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData("data:image/png;base64,")]
    [InlineData("data:image/jpeg;base64,")]
    public void Cancellation_is_not_converted_to_missing_image(string prefix)
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        Assert.Throws<OperationCanceledException>(() =>
            MarkdownDataImage.Decode(prefix + Png, 1024, cancellation.Token));
    }

    [Theory]
    [InlineData(0UL)]
    [InlineData(7UL)]
    public void Tiny_limits_reject_even_a_png_signature(ulong limit) =>
        Assert.Null(MarkdownDataImage.Decode("data:image/png;base64,iVBORw0KGgo=", limit, TestContext.Current.CancellationToken));

    [Theory]
    [InlineData("data:image/png;base64,")]
    [InlineData("data:image/jpeg;base64,")]
    public void Host_limit_applies_even_if_caller_requests_more(string prefix)
    {
        string payload = new('A', checked((int)((MarkdownDataImage.MaximumEncodedBytes / 3 + 1) * 4)));
        Assert.Null(MarkdownDataImage.Decode(prefix + payload, ulong.MaxValue, TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData("data:image/jpeg;base64,")]
    [InlineData("DATA:IMAGE/JPEG;BASE64,")]
    public void Jpeg_signature_gate_preserves_bytes_and_limit(string prefix)
    {
        // This exercises only the host format gate; WIC validates full images.
        byte[] bytes = [255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 255, 217];
        string payload = Convert.ToBase64String(bytes);
        Assert.Equal(bytes, MarkdownDataImage.Decode(prefix + payload, (ulong)bytes.Length, TestContext.Current.CancellationToken));
        Assert.Null(MarkdownDataImage.Decode(prefix + payload, (ulong)bytes.Length - 1, TestContext.Current.CancellationToken));
        Assert.Null(MarkdownDataImage.Decode("data:image/png;base64," + payload, 1024, TestContext.Current.CancellationToken));
        Assert.Null(MarkdownDataImage.Decode(prefix + Png, 1024, TestContext.Current.CancellationToken));
    }

    [Theory]
    [InlineData("data:image/jpeg;base64,")]
    [InlineData("data:image/jpeg;base64,/9g=")]
    [InlineData("data:image/jpeg;base64,AAAA")]
    [InlineData("data:image/jpeg;base64,!!!!")]
    [InlineData("data:image/jpeg;base64,/9j/\n")]
    [InlineData("data:image/jpeg;base64,%2F9j%2F")]
    [InlineData("data:image/jpeg;charset=utf-8;base64,/9j/")]
    [InlineData("data:image/jpg;base64,/9j/")]
    public void Invalid_jpeg_format_is_rejected(string destination) =>
        Assert.Null(MarkdownDataImage.Decode(destination, 1024, TestContext.Current.CancellationToken));

    [Theory]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(24_575)]
    [InlineData(24_576)]
    [InlineData(24_577)]
    public void Jpeg_padding_and_chunk_boundaries_preserve_bytes(int length)
    {
        byte[] bytes = new byte[length];
        bytes[0] = 255; bytes[1] = 216; bytes[2] = 255;
        for (int index = 3; index < length; ++index) bytes[index] = (byte)index;
        string destination = "data:image/jpeg;base64," + Convert.ToBase64String(bytes);
        Assert.Equal(bytes, MarkdownDataImage.Decode(destination, (ulong)length, TestContext.Current.CancellationToken));
        Assert.Null(MarkdownDataImage.Decode(destination, 2, TestContext.Current.CancellationToken));
    }
}
