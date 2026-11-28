// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Diagnostics/NativeStartupTrace.h"
#endif
#include <winrt/Windows.Devices.Input.h>
#include <winrt/Windows.UI.Input.h>

namespace winrt::Notepads::Markdown::implementation
{
    bool MarkdownView::IsSmoothScrollingEnabled() const
    {
        CheckUiThread();
        return _smoothScrollingEnabled;
    }

    void MarkdownView::IsSmoothScrollingEnabled(bool enabled)
    {
        CheckUiThread();
        _smoothScrollingEnabled = enabled;
        CancelWheelScroll();
    }

    void MarkdownView::SetScrollSettingsProvider(Markdown::IMarkdownScrollSettingsProvider const &provider)
    {
        CheckUiThread();
        _scrollSettingsProvider = provider;
        CancelWheelScroll();
    }

    void MarkdownView::CancelScrollAnimation()
    {
        CheckUiThread();
        CancelWheelScroll();
    }

    MarkdownView::ScrollIntent MarkdownView::CaptureScrollIntent() const
    {
        return {_previewScroll, _previewExtent, _previewSourceGeneration, _previewLayoutGeneration,
                _viewportGeometry.epoch, _previewToken->load(std::memory_order_acquire), _wheelScroll.Serial()};
    }

    bool MarkdownView::CurrentScrollIntent(ScrollIntent const &intent) const noexcept
    {
        return _loaded && _documentAccepted && intent.scroll && intent.extent && _previewScroll == intent.scroll &&
               _previewExtent == intent.extent && _previewSourceGeneration == intent.source &&
               _previewLayoutGeneration == intent.layout && _viewportGeometry.epoch == intent.geometry &&
               _previewToken->load(std::memory_order_acquire) == intent.token && _wheelScroll.Serial() == intent.serial;
    }

    uint64_t MarkdownView::CancelWheelScroll() noexcept
    {
        const bool active = _wheelScroll.Active();
        auto scroll = _previewScroll;
        // Publish retirement before ChangeView can synchronously raise events.
        const auto serial = _wheelScroll.Cancel();
        try
        {
            if (active && scroll && _owningDispatcher && _owningDispatcher.HasThreadAccess())
                scroll.ChangeView(nullptr, scroll.VerticalOffset(), nullptr, true);
        }
        catch (...)
        {
        }
        return serial;
    }

    MarkdownView::ScrollIntent MarkdownView::BeginImmediateScroll()
    {
        _pendingDiagramInspection.reset();
        auto intent = CaptureScrollIntent();
        intent.serial = CancelWheelScroll();
        return intent;
    }

    bool MarkdownView::ScrollExtentCurrent(ScrollIntent const &intent, double frameHeight) const
    {
        // Frame publication can precede ScrollViewer measurement of its new extent.
        const auto extent = std::max(ActualHeight(), frameHeight + 2 * _padding);
        const auto expected = std::max(0.0, extent - intent.scroll.ViewportHeight());
        const auto maximum = intent.scroll.ScrollableHeight();
        return std::isfinite(maximum) && maximum >= 0 &&
            std::abs(maximum - expected) * _viewportGeometry.scale <= 1.0;
    }

    double MarkdownView::VerticalScrollProgress() const
    {
        CheckUiThread();
        if (!IsPreviewLoaded() || !_documentAccepted || !_previewScroll || _previewScroll.Content() != _previewExtent)
            return -1;
        const auto maximum = _previewScroll.ScrollableHeight();
        const auto offset = _previewScroll.VerticalOffset();
        if (!std::isfinite(maximum) || maximum <= 0 || !std::isfinite(offset))
            return -1;
        const auto tolerance = std::min(0.5 / std::max(1.0, _viewportGeometry.scale), maximum * 0.5);
        if (offset <= tolerance)
            return 0;
        if (offset >= maximum - tolerance)
            return 1;
        return offset / maximum;
    }

    bool MarkdownView::TrySetVerticalScrollProgress(double progress)
    {
        CheckUiThread();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiScrollProgress, UiTrace::Phase::UiScrollProgressEnd, _previewLayoutGeneration};
#endif
        if (!std::isfinite(progress) || progress < 0 || progress > 1)
            throw hresult_invalid_argument();
        if (!IsPreviewLoaded() || !_documentAccepted || !_previewScroll || !CaptureViewportGeometry() ||
            _viewportTemplateState != ViewportTemplateState::Ready || _previewScroll.Content() != _previewExtent)
            return false;
        // A child preview owns its own scroll input. Main preview progress can
        // also originate in editor synchronization and must not claim that input.
        if (_noteParentView.get() && !BeginReadingScroll(_previewScroll, true))
            return false;
        const auto maximum = _previewScroll.ScrollableHeight();
        if (!std::isfinite(maximum) || maximum < 0)
            return false;
        _pendingReadingReveal.reset();
        CancelViewportRestore();
        auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || intent.scroll.ScrollableHeight() != maximum)
            return false;
        const auto target = progress * maximum;
        const auto offset = intent.scroll.VerticalOffset();
        if (target == offset || (progress > 0 && progress < 1 &&
            std::abs(target - offset) * _viewportGeometry.scale < 0.5))
            return true;
        CancelViewportRestore(target);
        const bool accepted = intent.scroll.ChangeView(nullptr, target, nullptr, true);
        if (!accepted && CurrentScrollIntent(intent))
            _imageUserScrollY.reset();
        return accepted && CurrentScrollIntent(intent);
    }

    event_token MarkdownView::VerticalScrollStarting(
        Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable> const &handler)
    {
        return _verticalScrollStarting.add(handler);
    }

    void MarkdownView::VerticalScrollStarting(event_token const &token) noexcept
    {
        _verticalScrollStarting.remove(token);
    }

    event_token MarkdownView::VerticalScrollChanged(
        Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Windows::Foundation::IInspectable> const &handler)
    {
        return _verticalScrollChanged.add(handler);
    }

    void MarkdownView::VerticalScrollChanged(event_token const &token) noexcept
    {
        _verticalScrollChanged.remove(token);
    }

    void MarkdownView::CancelViewportRestore(std::optional<double> target) noexcept
    {
        ++_imageScrollGeneration;
        _viewportToRestore.reset();
        _imageRestoringY.reset();
        _imageAnchor.reset();
        _imageAnchorNavigation.reset();
        _imageAnchorExtentPending = false;
        _imageUserScrollY = target;
    }

    bool MarkdownView::BeginReadingScroll(Windows::UI::Xaml::Controls::ScrollViewer const &scroll, bool movesVertically)
    {
        return movesVertically ? BeginVerticalReadingScroll(scroll) : ReadingScrollViewer() == scroll;
    }

    bool MarkdownView::BeginVerticalReadingScroll(Windows::UI::Xaml::Controls::ScrollViewer const &scroll,
                                                  std::optional<uint64_t> revealSerial)
    {
        if (ReadingScrollViewer() != scroll)
            return false;
        if (!revealSerial)
            _pendingReadingReveal.reset();
        auto ownsReveal = [&]
        {
            return !revealSerial || (_pendingReadingReveal && _pendingReadingReveal->serial == *revealSerial);
        };
        auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || intent.scroll != scroll || !ownsReveal())
            return false;
        CancelViewportRestore();
        // The same reveal may need another frame after a scroll observer relayouts.
        if (!revealSerial || !std::exchange(_pendingReadingReveal->scrollStartAnnounced, true))
        {
            try { _verticalScrollStarting(*this, nullptr); }
            catch (...) { /* A host observer must not invalidate scrolling. */ }
        }
        return CurrentScrollIntent(intent) && ownsReveal() && ReadingScrollViewer() == scroll;
    }

    void MarkdownView::AttachWheelScrollEvents()
    {
        if (!_loaded || !_previewScroll || !_previewExtent)
            return;
        if (!_wheelChanged)
            _wheelChanged = _previewExtent.PointerWheelChanged(
                auto_revoke, [weak = get_weak()](auto const &sender, auto const &args)
                {
                    if (auto owner = weak.get())
                        owner->OnPreviewWheel(sender, args);
                });
        if (_codeCopyButton && !_codeCopyWheel)
            _codeCopyWheel = _codeCopyButton.PointerWheelChanged(
                auto_revoke, [weak = get_weak()](auto const &sender, auto const &args)
                {
                    if (auto owner = weak.get())
                        owner->OnPreviewWheel(sender, args);
                });
        if (!_scrollPointerPressed)
        {
            // AddHandler takes a boxed delegate. Retain this exact box for
            // RemoveHandler; the delegate itself implements IUnknown only.
            _scrollPointerPressed = box_value(Windows::UI::Xaml::Input::PointerEventHandler(
                [weak = get_weak()](auto const &, auto const &)
                {
                    if (auto owner = weak.get())
                    {
                        owner->_pendingDiagramInspection.reset();
                        owner->_pendingReadingReveal.reset();
                        owner->CancelViewportRestore();
                        owner->CancelWheelScroll();
                    }
                }));
            AddHandler(Windows::UI::Xaml::UIElement::PointerPressedEvent(), _scrollPointerPressed, true);
        }
        if (!_scrollKeyDown)
            _scrollKeyDown = PreviewKeyDown(auto_revoke, [weak = get_weak()](auto const &, auto const &)
            {
                if (auto owner = weak.get())
                {
                    owner->_pendingReadingReveal.reset();
                    owner->CancelViewportRestore();
                    owner->CancelWheelScroll();
                }
            });
    }

    void MarkdownView::DetachWheelScrollEvents() noexcept
    {
        _wheelChanged.revoke();
        _codeCopyWheel.revoke();
        _scrollKeyDown.revoke();
        if (_scrollPointerPressed)
        {
            try
            {
                RemoveHandler(Windows::UI::Xaml::UIElement::PointerPressedEvent(), _scrollPointerPressed);
            }
            catch (...)
            {
            }
            _scrollPointerPressed = nullptr;
        }
        _wheelScrollableHeight = -1;
    }

    void MarkdownView::ObserveWheelScroll() noexcept
    {
        try
        {
            if (!_previewScroll)
                return;
            const double maximum = _previewScroll.ScrollableHeight();
            const bool extentChanged = _wheelScrollableHeight != maximum;
            const bool cancel = _wheelScrollableHeight >= 0 && extentChanged;
            _wheelScrollableHeight = maximum;
            if (cancel)
                CancelWheelScroll();
            else
                _wheelScroll.Observe(_previewScroll.VerticalOffset(), _viewportGeometry.scale);
            if (extentChanged)
            {
                try { _verticalScrollChanged(*this, box_value(true)); }
                catch (...) { /* A host observer must not invalidate scrolling. */ }
            }
        }
        catch (...)
        {
            CancelWheelScroll();
        }
    }

    void MarkdownView::OnPreviewWheel(Windows::Foundation::IInspectable const &sender,
                                     Windows::UI::Xaml::Input::PointerRoutedEventArgs const &args)
    {
        auto lifetime = get_strong();
        std::optional<ScrollIntent> claimed;
        try
        {
            if (args.Handled() || (sender != _previewExtent && sender != _codeCopyButton) || !_loaded || !_documentAccepted || !_previewScroll ||
                !CaptureViewportGeometry() || _viewportTemplateState != ViewportTemplateState::Ready ||
                _previewScroll.Content() != _previewExtent)
                return;
            _pendingDiagramInspection.reset();
            if (_diagramInspection)
            {
                auto point = args.GetCurrentPoint(*this);
                auto delta = point.Properties().MouseWheelDelta();
                if (!delta)
                    return;
                using Key = Windows::System::VirtualKey;
                using Modifier = Windows::System::VirtualKeyModifiers;
                auto modifiers = args.KeyModifiers();
                auto key = (modifiers & Modifier::Control) != Modifier::None
                    ? (delta > 0 ? Key::Add : Key::Subtract)
                    : point.Properties().IsHorizontalMouseWheel() || (modifiers & Modifier::Shift) != Modifier::None
                        ? (delta > 0 ? Key::Left : Key::Right) : (delta > 0 ? Key::Up : Key::Down);
                args.Handled(true);
                (void)ApplyDiagramInspectionKey(key);
                return;
            }
            const auto maximum = _previewScroll.ScrollableHeight();
            if (_previewScroll.VerticalScrollMode() == Windows::UI::Xaml::Controls::ScrollMode::Disabled ||
                !std::isfinite(maximum) || maximum <= 0)
                return;
            auto point = args.GetCurrentPoint(*this);
            if (point.PointerDevice().PointerDeviceType() != Windows::Devices::Input::PointerDeviceType::Mouse ||
                point.Properties().IsHorizontalMouseWheel() || !point.Properties().MouseWheelDelta())
                return;
            auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
            for (auto key : {Windows::System::VirtualKey::Control, Windows::System::VirtualKey::Shift,
                             Windows::System::VirtualKey::Menu})
            {
                if (window && (window.GetKeyState(key) & Windows::UI::Core::CoreVirtualKeyStates::Down) !=
                    Windows::UI::Core::CoreVirtualKeyStates::None)
                {
                    CancelWheelScroll();
                    return;
                }
            }
            _pendingReadingReveal.reset();
            if (!ReadingAvailable())
            {
                // Default ScrollViewer input can move while a new renderer is
                // pending. Retire the old reflow intent before that move.
                CancelViewportRestore();
                return;
            }
            ObserveWheelScroll();
            auto intent = CaptureScrollIntent();
            claimed = intent;
            auto provider = _scrollSettingsProvider;
            const uint32_t lines = provider ? provider.VerticalWheelScrollLines() : 3;
            if (!CurrentScrollIntent(intent) || args.Handled() || _scrollSettingsProvider != provider ||
                _previewScroll.Content() != intent.extent || !ReadingAvailable())
            {
                // The callback took ownership. Do not let the stale event fall
                // through to ScrollViewer and overwrite its newer position.
                args.Handled(true);
                return;
            }
            if (!lines)
            {
                intent.serial = CancelWheelScroll();
                claimed = intent;
                // Zero is a consuming policy, including synchronous takeover
                // from the cancellation callback. Never default-scroll it.
                args.Handled(true);
                return;
            }
            const auto delta = point.Properties().MouseWheelDelta();
            auto target = _wheelScroll.Plan(delta, intent.scroll.VerticalOffset(), intent.scroll.ScrollableHeight(),
                                            intent.scroll.ViewportHeight(), _lineHeight, lines);
            if (!target)
                return;
            // A nonanimated ChangeView can move before PointerWheelChanged
            // bubbles to the host. Give it the old alignment first.
            CancelViewportRestore(*target);
            try { _verticalScrollStarting(*this, nullptr); }
            catch (...) { /* A host observer must not invalidate scrolling. */ }
            const bool animated = _smoothScrollingEnabled && _systemColors && _systemColors.AnimationsEnabled();
            if (!CurrentScrollIntent(intent) || args.Handled())
            {
                args.Handled(true);
                return;
            }
            if (!animated)
            {
                intent.serial = CancelWheelScroll();
                claimed = intent;
                if (!CurrentScrollIntent(intent) || args.Handled())
                {
                    args.Handled(true);
                    return;
                }
            }
            // Publish before the external call. Reentrant Cancel/Reveal/new wheel
            // invalidates this serial even when no animation was active before.
            intent.serial = _wheelScroll.Publish(*target, delta);
            claimed = intent;
            const bool accepted = intent.scroll.ChangeView(nullptr, *target, nullptr, !animated);
            if (!CurrentScrollIntent(intent))
            {
                args.Handled(true);
                return;
            }
            if (!accepted)
            {
                _wheelScroll.Reject(intent.serial);
                _imageUserScrollY.reset();
                return;
            }
            if (!animated)
                _wheelScroll.Reject(intent.serial);
            args.Handled(true);
        }
        catch (...)
        {
            // Provider/XAML failures keep the default platform input path. A
            // callback's newer intent is never rolled back by this old handler.
            if (claimed && _wheelScroll.Serial() == claimed->serial)
                claimed->serial = CancelWheelScroll();
            if (claimed && !CurrentScrollIntent(*claimed))
            {
                try { args.Handled(true); } catch (...) {}
            }
        }
    }
}
