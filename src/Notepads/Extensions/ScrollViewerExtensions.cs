// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
// Source taken from: https://github.com/windows-toolkit/WindowsCommunityToolkit/pull/3220

namespace Notepads.Extensions
{
    using Windows.UI.Composition;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Hosting;

    /// <summary>
    /// Indicates an axis in the 2D space
    /// </summary>
    public enum Axis
    {
        /// <summary>
        /// The X axis (horizontal)
        /// </summary>
        X,

        /// <summary>
        /// The Y axis (vertical)
        /// </summary>
        Y
    }

    public static class ScrollViewerExtensions
    {
        public static ExpressionAnimation StartExpressionAnimation(
            this ScrollViewer scrollViewer,
            UIElement target,
            Axis axis)
        {
            return scrollViewer.StartExpressionAnimation(target, sourceAxis: axis, targetAxis: axis);
        }

        public static ExpressionAnimation StartExpressionAnimation(
            this ScrollViewer scrollViewer,
            UIElement target,
            Axis sourceAxis,
            Axis targetAxis)
        {
            CompositionPropertySet scrollSet = ElementCompositionPreview.GetScrollViewerManipulationPropertySet(scrollViewer);

            ExpressionAnimation animation = scrollSet.Compositor.CreateExpressionAnimation($"{nameof(scrollViewer)}.{nameof(UIElement.Translation)}.{sourceAxis}");
            animation.SetReferenceParameter(nameof(scrollViewer), scrollSet);

            Visual visual = ElementCompositionPreview.GetElementVisual(target);
            visual.StartAnimation($"{nameof(Visual.Offset)}.{targetAxis}", animation);

            return animation;
        }
    }
}