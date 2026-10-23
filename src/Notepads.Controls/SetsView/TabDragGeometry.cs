// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Controls;

// All inputs use local DIPs and increasing logical X; the control projects RTL once.
internal static class TabDragGeometry
{
    internal const double AdmissionDistance = 4;
    internal const double EscapeDistance = 16;
    private const double EdgeBand = 32;
    private const double MaximumEdgeSpeed = 480;

    internal static bool HasStarted(double dx, double dy) =>
        Math.Abs(dx) >= AdmissionDistance || Math.Abs(dy) >= AdmissionDistance;

    internal static bool Escapes(double y, double top, double bottom) =>
        y < top - EscapeDistance || y > bottom + EscapeDistance;

    internal static double SourceStart(double pointer, double grab, double first, double end, double width) =>
        Math.Clamp(pointer - grab, first, Math.Max(first, end - width));

    internal static int TargetIndex(ReadOnlySpan<double> centers, int source, double start, double width)
    {
        int target = source;
        if (start > centers[source] - width / 2)
        {
            while (target + 1 < centers.Length && start + width >= centers[target + 1]) target++;
        }
        else
        {
            while (target > 0 && start <= centers[target - 1]) target--;
        }
        return target;
    }

    internal static double NeighborOffset(int index, int source, int target, double sourceWidth) =>
        index >= target && index < source ? sourceWidth :
        index > source && index <= target ? -sourceWidth : 0;

    internal static double EdgeVelocity(double x, double left, double right)
    {
        double band = Math.Min(EdgeBand, (right - left) / 2);
        if (!(band > 0)) return 0;
        if (x < left + band) return -MaximumEdgeSpeed * Math.Clamp((left + band - x) / band, 0, 1);
        if (x > right - band) return MaximumEdgeSpeed * Math.Clamp((x - right + band) / band, 0, 1);
        return 0;
    }
}
