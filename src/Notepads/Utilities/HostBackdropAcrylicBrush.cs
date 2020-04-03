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
        public static readonly DependencyProperty TintOpacityProperty = DependencyProperty.Register(
            "TintOpacity",
            typeof(float),
            typeof(HostBackdropAcrylicBrush),
            new PropertyMetadata(0.0, OnTintOpacityChanged)
        );

        public float TintOpacity
        {
            get => (float)GetValue(TintOpacityProperty);
            set => SetValue(TintOpacityProperty, value);
        }

        private static void OnTintOpacityChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is HostBackdropAcrylicBrush brush)
            {
                if (brush.CompositionBrush is CompositionEffectBrush)
                {
                    TintOpacityToArithmeticCompositeEffectSourceAmount((float) e.NewValue,
                        _acrylicTintOpacityMinThreshold,
                        out var source1Amount,
                        out var source2Amount);
                    brush.CompositionBrush?.Properties.InsertScalar("Blender.Source1Amount", source1Amount);
                    brush.CompositionBrush?.Properties.InsertScalar("Blender.Source2Amount", source2Amount);
                }
                else if (brush.CompositionBrush is CompositionColorBrush)
                {
                    // Do nothing since we are falling back to CompositionColorBrush here
                    // TintOpacity only applies to the CompositionEffectBrush we created
                }
            }
        }

        public static readonly DependencyProperty LuminosityColorProperty = DependencyProperty.Register(
            "LuminosityColor",
            typeof(Color),
            typeof(HostBackdropAcrylicBrush),
            new PropertyMetadata(0.0, OnLuminosityColorChanged)
        );

        public Color LuminosityColor
        {
            get => (Color)GetValue(LuminosityColorProperty);
            set => SetValue(LuminosityColorProperty, value);
        }

        private static void OnLuminosityColorChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is HostBackdropAcrylicBrush brush)
            {
                if (brush.CompositionBrush is CompositionEffectBrush)
                {
                    brush.CompositionBrush?.Properties.InsertColor("LuminosityColor.Color", (Color)e.NewValue);
                }
                else if (brush.CompositionBrush is CompositionColorBrush colorBrush)
                {
                    colorBrush.Color = (Color) e.NewValue;
                }
            }
        }

        public Uri TextureUri { get; set; }

        private readonly SemaphoreSlim _semaphoreSlim = new SemaphoreSlim(1);

        private static readonly float _acrylicTintOpacityMinThreshold = 0.3f;

        protected override async void OnConnected()
        {
            await _semaphoreSlim.WaitAsync();
            if (CompositionBrush == null)
            {
                try
                {
                    CompositionBrush = BuildHostBackdropAcrylicBrush();
                }
                catch (Exception ex)
                {
                    // Fallback to color brush if unable to create HostBackdropAcrylicBrush
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

        public CompositionBrush BuildHostBackdropAcrylicBrush()
        {
            IGraphicsEffectSource backDropEffectSource = new CompositionEffectSourceParameter("Backdrop");

            var luminosityColorEffect = new ColorSourceEffect()
            {
                Name = "LuminosityColor",
                Color = LuminosityColor
            };

            TintOpacityToArithmeticCompositeEffectSourceAmount(TintOpacity, _acrylicTintOpacityMinThreshold,
                out var source1Amount,
                out var source2Amount);

            var graphicsEffect = new ArithmeticCompositeEffect
            {
                Name = "Blender",
                Source1 = backDropEffectSource,
                Source2 = luminosityColorEffect,
                MultiplyAmount = 0,
                Source1Amount = source1Amount,
                Source2Amount = source2Amount,
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

            var effectFactory = Window.Current.Compositor.CreateEffectFactory(graphicsEffect,
                new[] {  "LuminosityColor.Color", "Blender.Source1Amount", "Blender.Source2Amount" });
            CompositionEffectBrush brush = effectFactory.CreateBrush();

            var hostBackdropBrush = Window.Current.Compositor.CreateHostBackdropBrush();
            brush.SetSourceParameter("Backdrop", hostBackdropBrush);

            return brush;
        }

        private static void TintOpacityToArithmeticCompositeEffectSourceAmount(float tintOpacity, float minThreshold,
            out float source1Amount, out float source2Amount)
        {
            minThreshold = Math.Clamp(minThreshold, 0, 1);
            var adjustedTintOpacity = Math.Clamp(tintOpacity, 0, 1);
            adjustedTintOpacity = ((1 - minThreshold) * adjustedTintOpacity) + minThreshold;
            source1Amount = 1 - adjustedTintOpacity;
            source2Amount = adjustedTintOpacity;
        }
    }
}