// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Documents.IO;

namespace Notepads.UnitTests;

public sealed class FileNameTests
{
    [Theory]
    [InlineData("note.txt")]
    [InlineData("notes with spaces.md")]
    [InlineData("中文 😀.txt")]
    [InlineData(".gitignore")]
    [InlineData("CONSOLE.txt")]
    [InlineData("nul-history.txt")]
    [InlineData("archive.tar.gz")]
    public void OrdinaryNamesAreAccepted(string name)
    {
        Assert.True(FileNameRules.IsFilenameValid(name, out var error));
        Assert.Equal(InvalidFilenameError.None, error);
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData(" leading.txt")]
    [InlineData("trailing.txt ")]
    [InlineData("trailing.")]
    [InlineData(".")]
    [InlineData("..")]
    [InlineData("CON")]
    [InlineData("con.txt")]
    [InlineData("NUL.log")]
    [InlineData("COM1.txt")]
    [InlineData("lpt9")]
    [InlineData("AUX")]
    [InlineData("PRN")]
    [InlineData("child/name.txt")]
    [InlineData("child\\name.txt")]
    [InlineData("a:b.txt")]
    [InlineData("wild*.txt")]
    [InlineData("question?.txt")]
    [InlineData("quote\".txt")]
    [InlineData("pipe|.txt")]
    [InlineData("less<.txt")]
    [InlineData("greater>.txt")]
    [InlineData("embedded\0.txt")]
    [InlineData("control\u001f.txt")]
    public void NamesUnsuitableForTheWindowsFilePickerAreRejected(string name)
    {
        Assert.False(FileNameRules.IsFilenameValid(name, out var error));
        Assert.NotEqual(InvalidFilenameError.None, error);
    }

    [Fact]
    public void FilenameLengthBoundaryIsEnforced()
    {
        Assert.True(FileNameRules.IsFilenameValid(new string('a', 255), out _));
        Assert.False(FileNameRules.IsFilenameValid(new string('a', 256), out var error));
        Assert.Equal(InvalidFilenameError.TooLong, error);
    }
}
