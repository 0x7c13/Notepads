namespace Notepads.Utilities
{
    using System;
    using System.Collections.Generic;
    using System.Threading;
    using Windows.Graphics.Effects;
    using Windows.UI;
    using Windows.UI.Composition;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Media;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.Graphics.Canvas.Effects;

    public sealed class HostBackdropAcrylicBrush : XamlCompositionBrushBase
    {
        public Color LuminosityColor { get; set; }

        public float TintOpacity { get; set; }

        public Uri TextureUri { get; set; }

        private readonly SemaphoreSlim _semaphoreSlim = new SemaphoreSlim(1);

        protected override async void OnConnected()
        {
            await _semaphoreSlim.WaitAsync();
            if (CompositionBrush == null)
            {
                try
                {
                    CompositionBrush = Build(LuminosityColor);
                }
                catch (Exception ex)
                {
                    CompositionBrush = Window.Current.Compositor.CreateColorBrush(LuminosityColor);
                    Analytics.TrackEvent("FailedToBuildAcrylicBrush", 
                        new Dictionary<string, string> {{ "Exception", ex.ToString() }});
                }
            }
            _semaphoreSlim.Release();
            base.OnConnected();
        }

        protected override async void OnDisconnected()
        {
            await _semaphoreSlim.WaitAsync();
            if (CompositionBrush != null)
            {
                CompositionBrush.Dispose();
                CompositionBrush = null;
            }
            _semaphoreSlim.Release();
            base.OnDisconnected();
        }

        public CompositionBrush Build(
            Color luminosityColor)
        {
            var adjustedTintOpacity = Math.Clamp(TintOpacity, 0, 1);
            adjustedTintOpacity = (0.7f * adjustedTintOpacity) + 0.3f;

            IGraphicsEffectSource backDropEffectSource = new CompositionEffectSourceParameter("Backdrop");

            var luminosityColorEffect = new ColorSourceEffect()
            {
                Name = "LuminosityColor",
                Color = luminosityColor
            };

            var graphicsEffect = new ArithmeticCompositeEffect
            {
                Source1 = backDropEffectSource,
                Source2 = luminosityColorEffect,
                MultiplyAmount = 0,
                Source1Amount = 1 - adjustedTintOpacity,
                Source2Amount = adjustedTintOpacity,
                Offset = 0
            };

            //IGraphicsEffectSource noiseEffectSource = new CompositionEffectSourceParameter("Noise");
            //var noiseBorderEffect  = new BorderEffect()
            //{
            //    ExtendX = CanvasEdgeBehavior.Wrap,
            //    ExtendY = CanvasEdgeBehavior.Wrap,
            //    Source = noiseEffectSource,
            //};

            //var noiseOpacityEffect = new OpacityEffect()
            //{
            //    Name = "NoiseOpacity",
            //    Opacity = 0.5f,
            //    Source = noiseBorderEffect,
            //};

            //var finalEffect = new CrossFadeEffect()
            //{
            //    Name = "FadeInOut",
            //    Source1 = graphicsEffect,
            //    Source2 = noiseOpacityEffect
            //};

            var effectFactory = Window.Current.Compositor.CreateEffectFactory(graphicsEffect);
            CompositionEffectBrush brush = effectFactory.CreateBrush();

            var hostBackdropBrush = Window.Current.Compositor.CreateHostBackdropBrush();
            brush.SetSourceParameter("Backdrop", hostBackdropBrush);

            return brush;
        }
    }
}