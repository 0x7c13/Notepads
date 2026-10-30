// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Windows.UI;
using Windows.UI.Composition;

namespace Notepads.Presentation.Controls;

public sealed partial class DocumentLoadingPlaceholder
{
    internal CompositionRoundedRectangleGeometry[] TestLines => _lines.ToArray();
    internal Color TestBaseColor => _brush.ColorStops[0].Color;
    internal Color TestHighlightColor => _brush.ColorStops[1].Color;
    internal bool TestHasAnimation => _brush?.TryGetAnimationController("Offset.X") != null;
    internal bool TestHasResources => _visual != null || _brush != null || _settings != null || _window != null;
}
