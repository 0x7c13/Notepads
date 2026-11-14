// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Controls;

namespace Notepads.UnitTests;

public sealed class TabDragGeometryTests
{
    [Theory]
    [InlineData(0, 0, false)]
    [InlineData(3.99, -3.99, false)]
    [InlineData(4, 0, true)]
    [InlineData(0, -4, true)]
    public void ClickToleranceIsIndependentOfEscape(double dx, double dy, bool expected) =>
        Assert.Equal(expected, TabDragGeometry.HasStarted(dx, dy));

    [Theory]
    [InlineData(-16, false)]
    [InlineData(-16.01, true)]
    [InlineData(56, false)]
    [InlineData(56.01, true)]
    [InlineData(20, false)]
    public void EscapeIncludesMarginAroundMeasuredStrip(double y, bool expected) =>
        Assert.Equal(expected, TabDragGeometry.Escapes(y, 0, 40));

    [Theory]
    [InlineData(0, 39.9, 120, 0)]
    [InlineData(0, 40, 120, 1)]
    [InlineData(0, 179.9, 120, 1)]
    [InlineData(0, 180, 120, 2)]
    [InlineData(2, 160.1, 200, 2)]
    [InlineData(2, 160, 200, 1)]
    [InlineData(2, 60, 200, 0)]
    public void UnequalHeadersCrossOriginalMidpointsWithoutMutatingOrder(int source, double start, double width, int expected)
    {
        double[] centers = [60, 160, 300]; // Widths 120, 80, 200.
        Assert.Equal(expected, TabDragGeometry.TargetIndex(centers, source, start, width));
        Assert.Equal(new double[] { 60, 160, 300 }, centers);
    }

    [Fact]
    public void WideSourceCanReachBothEndsAcrossNarrowNeighborsAndReturn()
    {
        double[] right = [100, 220, 260];
        double[] left = [20, 60, 180];
        Assert.Equal(0, TabDragGeometry.TargetIndex(right, 0, 0, 200));
        Assert.Equal(1, TabDragGeometry.TargetIndex(right, 0, 20, 200));
        Assert.Equal(2, TabDragGeometry.TargetIndex(right, 0,
            TabDragGeometry.SourceStart(999, 100, 0, 280, 200), 200));
        Assert.Equal(2, TabDragGeometry.TargetIndex(left, 2, 80, 200));
        Assert.Equal(1, TabDragGeometry.TargetIndex(left, 2, 60, 200));
        Assert.Equal(0, TabDragGeometry.TargetIndex(left, 2,
            TabDragGeometry.SourceStart(-999, 100, 0, 280, 200), 200));
    }

    [Fact]
    public void GrabPointAndBothContentEndsConstrainSource()
    {
        Assert.Equal(130, TabDragGeometry.SourceStart(155, 25, 0, 400, 120));
        Assert.Equal(0, TabDragGeometry.SourceStart(-100, 25, 0, 400, 120));
        Assert.Equal(280, TabDragGeometry.SourceStart(1000, 25, 0, 400, 120));
        Assert.Equal(0, TabDragGeometry.SourceStart(100, 25, 0, 80, 120));
    }

    [Fact]
    public void OnlyCrossedNeighborsMakeRoomForTheSourceWidth()
    {
        Assert.Equal(0, TabDragGeometry.NeighborOffset(0, 0, 2, 120));
        Assert.Equal(-120, TabDragGeometry.NeighborOffset(1, 0, 2, 120));
        Assert.Equal(-120, TabDragGeometry.NeighborOffset(2, 0, 2, 120));
        Assert.Equal(0, TabDragGeometry.NeighborOffset(3, 0, 2, 120));
        Assert.Equal(200, TabDragGeometry.NeighborOffset(0, 2, 0, 200));
        Assert.Equal(200, TabDragGeometry.NeighborOffset(1, 2, 0, 200));
        Assert.Equal(0, TabDragGeometry.NeighborOffset(2, 2, 0, 200));
    }

    [Fact]
    public void EdgeVelocityIsSymmetricBoundedAndIdleAwayFromEdges()
    {
        Assert.Equal(0, TabDragGeometry.EdgeVelocity(160, 20, 300));
        Assert.Equal(0, TabDragGeometry.EdgeVelocity(52, 20, 300));
        Assert.Equal(-240, TabDragGeometry.EdgeVelocity(36, 20, 300));
        Assert.Equal(240, TabDragGeometry.EdgeVelocity(284, 20, 300));
        Assert.Equal(-480, TabDragGeometry.EdgeVelocity(-1000, 20, 300));
        Assert.Equal(480, TabDragGeometry.EdgeVelocity(1000, 20, 300));
        Assert.Equal(0, TabDragGeometry.EdgeVelocity(25, 20, 30));
        Assert.Equal(0, TabDragGeometry.EdgeVelocity(25, 20, 20));
    }
}
