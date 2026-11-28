// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Presentation.Controls.Markdown;

namespace Notepads.UnitTests;

public sealed class MarkdownDestinationTests
{
    [Theory]
    [InlineData(@"C:\docs\guide.md#intro", @"C:\docs\guide.md", "#intro")]
    [InlineData("C:/docs/my%20guide.md?v=1#intro", @"C:\docs\my guide.md", "#intro")]
    [InlineData("  C:/docs/my%20guide.md?v=1#intro", @"C:\docs\my guide.md", "#intro")]
    [InlineData("file:///C:/docs/my%20guide.md?v=1#intro", @"C:\docs\my guide.md", "#intro")]
    [InlineData("file:///C:/docs/my%2520guide.md#intro", @"C:\docs\my%20guide.md", "#intro")]
    [InlineData("guide%20%23intro.md?view=local#heading", "guide #intro.md", "#heading")]
    [InlineData(@"\\server\share\guide.md#intro", @"\\server\share\guide.md", "#intro")]
    [InlineData("//server/share/guide%20one.md#intro", @"\\server\share\guide one.md", "#intro")]
    [InlineData("  //server/share/guide%20one.md#intro", @"\\server\share\guide one.md", "#intro")]
    [InlineData("file://server/share/guide%20one.md#intro", @"\\server\share\guide one.md", "#intro")]
    [InlineData("%5C%5Cserver%5Cshare%5Cguide.md", @"\\server\share\guide.md", null)]
    [InlineData("images/icon.png", @"images\icon.png", null)]
    [InlineData(" draft.md", " draft.md", null)]
    [InlineData(@"\\GLOBALROOT\share\guide.md", @"\\GLOBALROOT\share\guide.md", null)]
    public void File_destinations_preserve_markdown_suffixes_and_decode_once(string input, string path, string fragment)
    {
        Assert.True(MarkdownDestination.TryParse(input, false, true, out var actualPath, out var remote, out var actualFragment));
        Assert.Equal(path, actualPath);
        Assert.Equal(fragment, actualFragment);
        Assert.Null(remote);
    }

    [Theory]
    [InlineData(@"\\?\C:\docs\guide.md")]
    [InlineData(@"  \\?\C:\docs\guide.md")]
    [InlineData(@"\\.\C:\docs\guide.md")]
    [InlineData(@"\??\C:\docs\guide.md")]
    [InlineData(@"\\??\C:\docs\guide.md")]
    [InlineData(@"\\?\GLOBALROOT\Device\HarddiskVolume1\guide.md")]
    [InlineData("//??/C:/docs/guide.md")]
    [InlineData("%5C%5C%3F%3F%5CC%3A%5Cdocs%5Cguide.md")]
    [InlineData("%5C%5C%3F%5CGLOBALROOT%5CDevice%5CHarddiskVolume1%5Cguide.md")]
    [InlineData(@"\guide.md")]
    [InlineData("/guide.md")]
    [InlineData("C:guide.md")]
    [InlineData("custom:guide")]
    public void Device_and_process_relative_paths_are_rejected_for_links_and_images(string input)
    {
        Assert.False(MarkdownDestination.TryParse(input, false, true, out _, out _, out _));
        Assert.False(MarkdownDestination.TryParse(input, false, false, out _, out _, out _));
    }

    [Fact]
    public void Wiki_paths_keep_literal_escapes_and_heading_titles()
    {
        Assert.True(MarkdownDestination.TryParse("target 25%20 会议 1.2#安装 指南", true, true,
            out var path, out var remote, out var fragment));
        Assert.Equal("target 25%20 会议 1.2", path);
        Assert.Equal("安装 指南", fragment);
        Assert.Null(remote);
    }

    [Fact]
    public void Only_links_admit_mailto_and_http_images_keep_their_complete_uri()
    {
        Assert.True(MarkdownDestination.TryParse("mailto:me@example.test", false, true, out var path, out var remote, out _));
        Assert.Null(path);
        Assert.Equal("mailto", remote.Scheme);
        Assert.False(MarkdownDestination.TryParse("mailto:me@example.test", false, false, out _, out _, out _));
        const string image = "https://example.test/image.png?v=1#picture";
        Assert.True(MarkdownDestination.TryParse(image, false, false, out path, out remote, out _));
        Assert.Null(path);
        Assert.Equal(image, remote.AbsoluteUri);
    }
}
