namespace Notepads.Brushes
{
    using System;
    using System.Collections.Generic;
    using System.Numerics;
    using System.Threading;
    using System.Threading.Tasks;
    using Windows.Foundation;
    using Windows.Graphics.DirectX;
    using Windows.Graphics.Display;
    using Windows.Graphics.Effects;
    using Windows.Graphics.Imaging;
    using Windows.System;
    using Windows.System.Power;
    using Windows.UI;
    using Windows.UI.Composition;
    using Windows.UI.ViewManagement;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Media;
    using Microsoft.AppCenter.Analytics;
    using Microsoft.Graphics.Canvas;
    using Microsoft.Graphics.Canvas.Effects;
    using Microsoft.Graphics.Canvas.UI.Composition;
    using Notepads.Controls.Helpers;

    public sealed class HostBackdropAcrylicBrush : XamlCompositionBrushBase
    {
        public static readonly DependencyProperty TintOpacityProperty = DependencyProperty.Register(
            "TintOpacity",
            typeof(float),
            typeof(HostBackdropAcrylicBrush),
            new PropertyMetadata(0.0f, OnTintOpacityChanged)
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
                    TintOpacityToArithmeticCompositeEffectSourceAmount((float)e.NewValue,
                        _acrylicTintOpacityMinThreshold,
                        out var source1Amount,
                        out var source2Amount);
                    brush.CompositionBrush?.Properties.InsertScalar("LuminosityBlender.Source1Amount", source1Amount);
                    brush.CompositionBrush?.Properties.InsertScalar("LuminosityBlender.Source2Amount", source2Amount);
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
            new PropertyMetadata(Colors.Transparent, OnLuminosityColorChanged)
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
                    if (brush.CompositionBrush?.Properties.TryGetColor("LuminosityColor.Color", out var currentColor) == CompositionGetValueStatus.Succeeded)
                    {
                        var easing = Window.Current.Compositor.CreateLinearEasingFunction();
                        var animation = Window.Current.Compositor.CreateColorKeyFrameAnimation();
                        animation.InsertKeyFrame(0.0f, currentColor);
                        animation.InsertKeyFrame(1.0f, (Color)e.NewValue, easing);
                        animation.Duration = TimeSpan.FromMilliseconds(167);
                        brush.CompositionBrush.StartAnimation("LuminosityColor.Color", animation);
                    }
                    else
                    {
                        brush.CompositionBrush?.Properties.InsertColor("LuminosityColor.Color", (Color)e.NewValue);
                    }
                }
                else if (brush.CompositionBrush is CompositionColorBrush colorBrush)
                {
                    colorBrush.Color = (Color)e.NewValue;
                }
            }
        }

        public Uri TextureUri { get; set; }

        private readonly SemaphoreSlim _semaphoreSlim = new SemaphoreSlim(1);

        private static readonly UISettings UISettings = new UISettings();

        private static readonly float _acrylicTintOpacityMinThreshold = 0.35f;

        private readonly DispatcherQueue _dispatcherQueue;

        private static readonly Dictionary<Uri, CanvasBitmap> CanvasBitmapCache = new Dictionary<Uri, CanvasBitmap>();

        public HostBackdropAcrylicBrush()
        {
            _dispatcherQueue = DispatcherQueue.GetForCurrentThread();
        }

        protected override async void OnConnected()
        {
            if (CompositionBrush == null)
            {
                await BuildInternalAsync();
            }
            base.OnConnected();
        }

        private async Task BuildInternalAsync()
        {
            await _semaphoreSlim.WaitAsync();
            try
            {
                
                if (PowerManager.EnergySaverStatus == EnergySaverStatus.On || !UISettings.AdvancedEffectsEnabled)
                {
                    CompositionBrush = Window.Current.Compositor.CreateColorBrush(LuminosityColor);
                }
                else
                {
                    CompositionBrush = await BuildHostBackdropAcrylicBrushAsync();   
                }

                // Register energy saver event
                PowerManager.EnergySaverStatusChanged -= OnEnergySaverStatusChanged;
                PowerManager.EnergySaverStatusChanged += OnEnergySaverStatusChanged;

                // Register system level transparency effects settings change event
                UISettings.AdvancedEffectsEnabledChanged -= OnAdvancedEffectsEnabledChanged;
                UISettings.AdvancedEffectsEnabledChanged += OnAdvancedEffectsEnabledChanged;
            }
            catch (Exception ex)
            {
                // Fallback to color brush if unable to create HostBackdropAcrylicBrush
                CompositionBrush = Window.Current.Compositor.CreateColorBrush(LuminosityColor);
                Analytics.TrackEvent("FailedToBuildAcrylicBrush",
                    new Dictionary<string, string> {{"Exception", ex.ToString()}});
            }
            finally
            {
                _semaphoreSlim.Release();
            }
        }

        private async void OnEnergySaverStatusChanged(object sender, object e)
        {
            await _dispatcherQueue.ExecuteOnUIThreadAsync(async () =>
            {
                await BuildInternalAsync();
            });
        }

        private async void OnAdvancedEffectsEnabledChanged(UISettings sender, object args)
        {
            await _dispatcherQueue.ExecuteOnUIThreadAsync(async () =>
            {
                await BuildInternalAsync();
            });
        }

        protected override async void OnDisconnected()
        {
            await _semaphoreSlim.WaitAsync();
            if (CompositionBrush != null)
            {
                PowerManager.EnergySaverStatusChanged -= OnEnergySaverStatusChanged;
                UISettings.AdvancedEffectsEnabledChanged -= OnAdvancedEffectsEnabledChanged;

                CompositionBrush.Dispose();
                CompositionBrush = null;
            }
            _semaphoreSlim.Release();
            base.OnDisconnected();
        }

        public async Task<CompositionBrush> BuildHostBackdropAcrylicBrushAsync()
        {
            var luminosityColorEffect = new ColorSourceEffect()
            {
                Name = "LuminosityColor",
                Color = LuminosityColor
            };

            TintOpacityToArithmeticCompositeEffectSourceAmount(TintOpacity, _acrylicTintOpacityMinThreshold,
                out var source1Amount,
                out var source2Amount);

            var luminosityBlendingEffect = new ArithmeticCompositeEffect
            {
                Name = "LuminosityBlender",
                Source1 = new CompositionEffectSourceParameter("Backdrop"),
                Source2 = luminosityColorEffect,
                MultiplyAmount = 0,
                Source1Amount = source1Amount,
                Source2Amount = source2Amount,
                Offset = 0
            };

            var noiseBorderEffect = new BorderEffect()
            {
                ExtendX = CanvasEdgeBehavior.Wrap,
                ExtendY = CanvasEdgeBehavior.Wrap,
                Source = new CompositionEffectSourceParameter("Noise"),
            };

            var noiseBlendingEffect = new BlendEffect()
            {
                Name = "NoiseBlender",
                Mode = BlendEffectMode.Overlay,
                Background = luminosityBlendingEffect,
                Foreground = noiseBorderEffect
            };

            var noiseImageBrush = await LoadImageBrushAsync(TextureUri);

            IGraphicsEffect finalEffect;

            if (noiseImageBrush == null)
            {
                finalEffect = luminosityBlendingEffect;
            }
            else
            {
                finalEffect = noiseBlendingEffect;
            }

            CompositionEffectFactory effectFactory = Window.Current.Compositor.CreateEffectFactory(finalEffect,
                new[]
                {
                    "LuminosityColor.Color",
                    "LuminosityBlender.Source1Amount",
                    "LuminosityBlender.Source2Amount"
                });

            CompositionEffectBrush brush = effectFactory.CreateBrush();

            var hostBackdropBrush = Window.Current.Compositor.CreateHostBackdropBrush();
            brush.SetSourceParameter("Backdrop", hostBackdropBrush);

            if (noiseImageBrush != null)
            {
                brush.SetSourceParameter("Noise", noiseImageBrush);
            }

            return brush;
        }

        private async Task<CompositionSurfaceBrush> LoadImageBrushAsync(Uri textureUri)
        {
            try
            {
                CanvasDevice sharedDevice = CanvasDevice.GetSharedDevice();
                DisplayInformation display = DisplayInformation.GetForCurrentView();
                float dpi = display.LogicalDpi;

                CanvasBitmap bitmap;
                if (CanvasBitmapCache.ContainsKey(textureUri))
                {
                    bitmap = CanvasBitmapCache[textureUri];
                }
                else
                {
                    bitmap = await CanvasBitmap.LoadAsync(sharedDevice, textureUri, dpi >= 96 ? dpi : 96);
                    CanvasBitmapCache[textureUri] = bitmap;
                }

                CompositionGraphicsDevice device = CanvasComposition.CreateCompositionGraphicsDevice(Window.Current.Compositor, sharedDevice);
                CompositionDrawingSurface surface = device.CreateDrawingSurface(default, DirectXPixelFormat.B8G8R8A8UIntNormalized, DirectXAlphaMode.Premultiplied);

                Size size = bitmap.Size;
                Size sizeInPixels = new Size(bitmap.SizeInPixels.Width, bitmap.SizeInPixels.Height);
                CanvasComposition.Resize(surface, sizeInPixels);

                using (CanvasDrawingSession session = CanvasComposition.CreateDrawingSession(surface, new Rect(0, 0, sizeInPixels.Width, sizeInPixels.Height), dpi))
                {
                    session.Clear(Color.FromArgb(0, 0, 0, 0));
                    session.DrawImage(bitmap, new Rect(0, 0, size.Width, size.Height), new Rect(0, 0, size.Width, size.Height));
                    session.EffectTileSize = new BitmapSize { Width = (uint)size.Width, Height = (uint)size.Height };

                    CompositionSurfaceBrush brush = surface.Compositor.CreateSurfaceBrush(surface);
                    brush.Stretch = CompositionStretch.None;
                    double pixels = display.RawPixelsPerViewPixel;
                    if (pixels > 1)
                    {
                        brush.Scale = new Vector2((float)(1 / pixels));
                        brush.BitmapInterpolationMode = CompositionBitmapInterpolationMode.NearestNeighbor;
                    }
                    return brush;
                }
            }
            catch (Exception ex)
            {
                Analytics.TrackEvent("FailedToLoadImageBrush",
                    new Dictionary<string, string> { { "Exception", ex.ToString() } });
                return null;
            }
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