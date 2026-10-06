// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

// These deliberately illegal dependencies exercise the checker on its own PE.
namespace Notepads.Presentation.Probe
{
    internal interface IView { }
    internal sealed class View : IView
    {
        public static void Show() { }
    }
    [AttributeUsage(AttributeTargets.Class)]
    internal sealed class ViewAttribute(Type type) : Attribute
    {
        public Type Target { get; } = type;
    }
}

namespace Notepads.Features.Sessions.Storage
{
    internal sealed class ForbiddenField
    {
        public Presentation.Probe.View? View = null;
    }
    internal sealed class ForbiddenSignature
    {
        public List<Presentation.Probe.View>? GetViews() => null;
    }
    internal sealed class ForbiddenProperty
    {
        public Presentation.Probe.View? View { get; set; }
    }
    internal sealed class ForbiddenCall
    {
        public void Call() => Presentation.Probe.View.Show();
    }
    [Presentation.Probe.View(typeof(Presentation.Probe.View))]
    internal sealed class ForbiddenAttribute { }
    internal sealed class ForbiddenInterface : Presentation.Probe.IView { }
}

namespace Windows.UI.Xaml.Controls
{
    internal sealed class TextBox { }
}

namespace Notepads.Features.Preferences
{
    internal sealed class ForbiddenXaml
    {
        public Windows.UI.Xaml.Controls.TextBox? View = null;
    }
}

namespace Notepads.Utilities
{
    internal sealed class ForbiddenUnknownNamespace { }
}
