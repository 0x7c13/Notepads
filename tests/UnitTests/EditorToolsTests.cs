// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Features.Documents.Text;

namespace NotepadsUnitTests;

public sealed class EditorToolsTests
{
    [Fact]
    public void SortIsOrdinalAndPreservesExactLines()
    {
        var sorted = SelectedLineOperations.Transform("b\rA\rb\r中文😀\r\0", SelectedLineOperation.SortAscending);
        Assert.Equal("\0\rA\rb\rb\r中文😀", sorted);
        var reverse = SelectedLineOperations.Transform("b\rA\rb", SelectedLineOperation.SortDescending);
        Assert.Equal("b\rb\rA", reverse);
    }

    [Fact]
    public void DedupKeepsFirstAndDoesNotFoldCaseOrWhitespace()
    {
        var result = SelectedLineOperations.Transform("a\rA\ra\r\r \r\rA", SelectedLineOperation.RemoveDuplicates);
        Assert.Equal("a\rA\r\r ", result);
    }

    [Fact]
    public void OperationsRejectOversizeAndCancellation()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() => SelectedLineOperations.Transform(
            new string('\r', SelectedLineOperations.MaximumLines), SelectedLineOperation.SortAscending));
        Assert.Throws<OperationCanceledException>(() => SelectedLineOperations.Transform("b\ra",
            SelectedLineOperation.SortAscending, new CancellationToken(true)));
    }

}
