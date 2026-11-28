// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Control/MarkdownSelection.h"
#include "Control/MarkdownLinkRequestedEventArgs.h"
#include "Automation/ReadingObjectProvider.h"
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.ApplicationModel.DataTransfer.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.Devices.Input.h>
#include <winrt/Windows.UI.Input.h>
#include <winrt/Windows.Foundation.Metadata.h>

namespace winrt::Notepads::Markdown::implementation
{
    using namespace Windows::Foundation;
    using namespace Windows::UI::Xaml;
    using namespace Windows::UI::Xaml::Controls;
    using namespace Windows::UI::Xaml::Media;
    using namespace Windows::UI::Xaml::Media::Imaging;

    void MarkdownView::MoveReadingCaret(uint32_t position, bool extend, bool trailingCaret)
    {
        auto lifetime = get_strong();
        if (!ReadingAvailable())
            return;
        auto action = CaptureReadingAction();
        auto const &navigation = *action.frame->navigation;
        position = navigation.Snap(position, true);
        CancelViewportRestore();
        auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || !CurrentReadingAction(action))
            return;
        auto anchor = extend ? _selectionAnchor : position;
        if (!UpdateSelection(anchor, position, true, SelectionMotion::Other, trailingCaret) ||
            !CurrentReadingAction(action) || !CurrentScrollIntent(intent) ||
            _selectionAnchor != anchor || _selectionActive != position)
            return;
        auto y = (navigation.LineCount() ? navigation.Line(navigation.LineAt(position, trailingCaret)).y : 0.0) +
                 _padding;
        if (_previewScroll && (y < _previewScroll.VerticalOffset() ||
                               y + _lineHeight > _previewScroll.VerticalOffset() + _previewScroll.ViewportHeight()))
        {
            const auto target = std::max(0.0, y - _previewScroll.ViewportHeight() * 0.4);
            CancelViewportRestore(target);
            _previewScroll.ChangeView(nullptr, target, nullptr, true);
        }
    }
    std::optional<size_t> MarkdownView::HitReadingLink(::Notepads::Markdown::Native::PreviewFrame const &frame,
                                                       ::Notepads::Markdown::Native::ReadingHit const &hit)
    {
        if (!frame.descriptors || !hit.inside || hit.cluster.start >= hit.cluster.end)
            return {};
        return frame.descriptors->LinkAt(hit.cluster.start);
    }
    std::optional<::Notepads::Markdown::Native::ReadingActionTarget> MarkdownView::HitReadingAction(
        ::Notepads::Markdown::Native::ReadingHit const &hit, Point point) const
    {
        using Kind = ::Notepads::Markdown::Native::ReadingActionKind;
        auto viewportPoint = ControlToViewport(point);
        auto width = _presented.geometry.width;
        auto height = _presented.geometry.height;
        if (viewportPoint.X < 0 || viewportPoint.Y < 0 || viewportPoint.X >= width ||
            viewportPoint.Y >= height || !ReadingAvailable())
            return {};
        auto snapshot = _automationSnapshot;
        if (hit.inside && hit.cluster.start < hit.cluster.end)
        {
            auto const &objects = snapshot->descriptors->objects;
            auto object = std::lower_bound(objects.begin(), objects.end(), hit.cluster.start,
                                           [](auto const &value, uint32_t start) { return value.text.start < start; });
            if (object != objects.end() && object->text.start == hit.cluster.start && object->text.end == hit.cluster.end)
            {
                auto index = static_cast<size_t>(object - objects.begin());
                if (index <= UINT32_MAX && ReadingTaskEnabled(*snapshot, index))
                    return ::Notepads::Markdown::Native::ReadingActionTarget{Kind::Task, static_cast<uint32_t>(index)};
            }
            if (auto link = HitReadingLink(*_presented.frame, hit); link && *link <= UINT32_MAX)
                return ::Notepads::Markdown::Native::ReadingActionTarget{Kind::Link, static_cast<uint32_t>(*link)};
        }
        if (auto disclosure = ::Notepads::Markdown::Native::NativeDocumentRenderer::DisclosureAt(
                *_presented.frame, viewportPoint.X - _padding, viewportPoint.Y - _padding))
            return ::Notepads::Markdown::Native::ReadingActionTarget{Kind::Disclosure, *disclosure};
        return {};
    }
    bool MarkdownView::IsReadingPointerOnChrome(IInspectable const &originalSource) const
    {
        auto source = originalSource.try_as<DependencyObject>();
        while (source && source != *this)
        {
            if (source == _codeCopyButton || source.try_as<Windows::UI::Xaml::Controls::Primitives::ScrollBar>() ||
                source.try_as<Markdown::MarkdownView>())
                return true;
            source = VisualTreeHelper::GetParent(source);
        }
        return false;
    }
    std::optional<size_t> MarkdownView::ReadingLinkAtPointer(PresentedViewport const &viewport) const noexcept
    {
        if (!_readingLinkPoint || _dragging || !viewport.frame || !viewport.frame->descriptors ||
            viewport.frame->descriptors->links.empty())
            return {};
        try
        {
            auto point = TransformToVisual(nullptr).Inverse().TransformPoint(*_readingLinkPoint);
            auto x = point.X - static_cast<float>(viewport.geometry.x);
            auto y = point.Y - static_cast<float>(viewport.geometry.y);
            if (x < 0 || y < 0 || x >= viewport.geometry.width || y >= viewport.geometry.height)
                return {};
            auto hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTest(*viewport.frame, x - _padding,
                                                                                  y - _padding);
            return hit ? HitReadingLink(*viewport.frame, *hit) : std::nullopt;
        }
        catch (...)
        {
            return {};
        }
    }

    std::optional<size_t> MarkdownView::UpdateReadingLinkCursor() noexcept
    {
        const bool available = ReadingAvailable();
        auto hovered = available ? ReadingLinkAtPointer(_presented) : std::nullopt;
        SetReadingLinkCursor(hovered.has_value());
        if (!available || _linkHoverDrawQueued || hovered == _presented.hoveredLink)
            return hovered;

        // Geometry updates may reach this method during a draw. Queue one repaint after
        // the current event, using whichever link is under the pointer at that time.
        _linkHoverDrawQueued = true;
        try
        {
            _owningDispatcher.RunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal, [weak = get_weak()]
            {
                if (auto owner = weak.get())
                {
                    owner->_linkHoverDrawQueued = false;
                    if (owner->ReadingAvailable() &&
                        owner->ReadingLinkAtPointer(owner->_presented) != owner->_presented.hoveredLink)
                        owner->DrawPreview();
                }
            });
        }
        catch (...)
        {
            _linkHoverDrawQueued = false;
        }
        return hovered;
    }
    void MarkdownView::SetReadingLinkCursor(bool overLink) noexcept
    {
        if (!overLink && !_readingLinkCursor)
            return;
        try
        {
            if (!_owningDispatcher || !_owningDispatcher.HasThreadAccess())
                return;
            using namespace Windows::UI::Core;
            auto window = CoreWindow::GetForCurrentThread();
            if (!window)
                return;
            auto current = window.PointerCursor();
            if (overLink)
            {
                if (_readingLinkCursor && current == _readingLinkCursor)
                    return;
                CoreCursor hand{CoreCursorType::Hand, 0};
                window.PointerCursor(hand);
                _readingLinkCursor = hand;
                _previousReadingCursor = current;
            }
            else
            {
                auto hand = std::exchange(_readingLinkCursor, nullptr);
                auto previous = std::exchange(_previousReadingCursor, nullptr);
                // Another control in this window may already own the cursor.
                if (current == hand)
                    window.PointerCursor(previous);
            }
        }
        catch (...)
        {
        }
    }
    void MarkdownView::OnReadingPointerPressed(IInspectable const &,
                                               Windows::UI::Xaml::Input::PointerRoutedEventArgs const &args)
    {
        auto lifetime = get_strong();
        ClearReadingHover();
        auto point = args.GetCurrentPoint(*this);
        if (!point.Properties().IsLeftButtonPressed())
            return;
        if (IsReadingPointerOnChrome(args.OriginalSource()))
            return;
        _pendingReadingReveal.reset();
        _pendingDiagramInspection.reset();
        if (_diagramInspection)
        {
            _diagramInspection.reset();
            DrawPreview();
            args.Handled(true);
            return;
        }
        if (!ReadingAvailable())
            return;
        std::optional<uint64_t> drag;
        auto cancelGesture = [&]() noexcept
        {
            // A synchronous callback can establish a newer gesture using the
            // same pointer. Cleanup belongs only to this invocation's serial.
            if (drag && *drag == _readingDragGeneration)
            {
                try
                {
                    StopReadingDrag();
                }
                catch (...)
                {
                }
            }
        };
        try
        {
            auto action = CaptureReadingAction();
            auto at = point.Position();
            auto local = ControlToViewport(at);
            if (local.X < 0 || local.Y < 0 || local.X >= _presented.geometry.width || local.Y >= _presented.geometry.height) return;
            auto hit =
                ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTest(*action.frame, local.X - _padding, local.Y - _padding);
            if (!hit)
                return;
            using Modifier = Windows::System::VirtualKeyModifiers;
            if (hit->inside && (args.KeyModifiers() & Modifier::Control) != Modifier::None)
            {
                auto const &objects = action.snapshot->descriptors->objects;
                auto diagram = std::lower_bound(objects.begin(), objects.end(), hit->cluster.start,
                    [](auto const &object, uint32_t start) { return object.text.start < start; });
                if (diagram != objects.end() && diagram->text.start == hit->cluster.start &&
                    diagram->text.end == hit->cluster.end &&
                    diagram->kind == ::Notepads::Markdown::Native::ReadingObjectKind::Diagram)
                {
                    auto index = static_cast<size_t>(diagram - objects.begin());
                    if (ReadingDiagramEnabled(*action.snapshot, index))
                    {
                        args.Handled(true);
                        InvokeReadingDiagram(*action.snapshot, index);
                        return;
                    }
                }
            }
            auto pointer = args.Pointer();
            auto pointerId = pointer.PointerId();
            auto taskGeneration = _taskInteractionGeneration;
            auto target = HitReadingAction(*hit, at).value_or(::Notepads::Markdown::Native::ReadingActionTarget{});
            bool taskCluster{};
            {
                auto const &objects = action.snapshot->descriptors->objects;
                auto object =
                    std::lower_bound(objects.begin(), objects.end(), hit->cluster.start,
                                     [](auto const &value, uint32_t start) { return value.text.start < start; });
                taskCluster = object != objects.end() && object->text.start == hit->cluster.start &&
                              object->text.end == hit->cluster.end &&
                              object->kind == ::Notepads::Markdown::Native::ReadingObjectKind::Task &&
                               !object->staticHtmlCheckbox && object->hasTaskAction;
            }
            auto sameAction = [&]
            {
                return CurrentReadingAction(action) && (!taskCluster || taskGeneration == _taskInteractionGeneration) &&
                       HitReadingAction(*hit, at).value_or(::Notepads::Markdown::Native::ReadingActionTarget{}) == target;
            };
            auto previousDrag = _readingDragGeneration;
            auto previousPointer = _readingPointerId;
            auto previousDragging = _dragging;
            Focus(Windows::UI::Xaml::FocusState::Pointer);
            if (!sameAction() || previousDrag != _readingDragGeneration || previousPointer != _readingPointerId ||
                previousDragging != _dragging)
                return;
            if (_readingDragGeneration == UINT64_MAX)
                throw hresult_error(E_BOUNDS);
            drag = ++_readingDragGeneration;
            _readingPointerId = pointerId;
            _dragging = true;
            UpdateReadingLinkCursor();
            _dragExceeded = false;
            _dragStart = _lastDragPoint = at;
            _pressedAction = target;
            _readingPressedFrame = action.frame;
            _readingPressedTaskGeneration = taskGeneration;
            _readingPressedPresentation = action.presentation;
            if (!CapturePointer(pointer))
            {
                cancelGesture();
                return;
            }
            if (!sameAction() || *drag != _readingDragGeneration || _readingPointerId != pointerId || !_dragging ||
                _pressedAction != target || _readingPressedFrame.lock() != action.frame ||
                _readingPressedTaskGeneration != taskGeneration)
            {
                cancelGesture();
                return;
            }
            if (_pressedAction.kind == ::Notepads::Markdown::Native::ReadingActionKind::Task)
            {
                _focusedAction = _pressedAction;
                InvalidateVerticalNavigation();
                DrawPreview();
                args.Handled(true);
                return;
            }
            auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
            auto shift = window &&
                (window.GetKeyState(Windows::System::VirtualKey::Shift) &
                 Windows::UI::Core::CoreVirtualKeyStates::Down) != Windows::UI::Core::CoreVirtualKeyStates::None;
            _focusedAction = {};
            if (!UpdateSelection(shift ? _selectionAnchor : hit->position, hit->position,
                                 true, SelectionMotion::Other, hit->trailingCaret) &&
                *drag == _readingDragGeneration)
                cancelGesture();
            args.Handled(true);
        }
        catch (...)
        {
            cancelGesture();
        }
    }
    void MarkdownView::StopReadingDrag()
    {
        if (_dragging)
            _readingLinkPoint.reset();
        _dragging = _dragExceeded = false;
        _readingPointerId.reset();
        _pressedAction = {};
        _readingPressedFrame.reset();
        _readingPressedTaskGeneration = 0;
        if (_readingDragTimer)
            _readingDragTimer.Stop();
        ReleasePointerCaptures();
    }
    bool MarkdownView::ContinueReadingDrag()
    {
        auto lifetime = get_strong();
        if (!_dragging || !ReadingAvailable())
        {
            if (_readingDragTimer)
                _readingDragTimer.Stop();
            return false;
        }
        auto drag = _readingDragGeneration;
        auto pointer = _readingPointerId;
        auto start = _dragStart;
        auto point = _lastDragPoint;
        auto sameDrag = [&]
        {
            return _dragging && pointer && _readingPointerId == pointer && drag == _readingDragGeneration &&
                   start.X == _dragStart.X && start.Y == _dragStart.Y && point.X == _lastDragPoint.X &&
                   point.Y == _lastDragPoint.Y;
        };
        _dragExceeded = _dragExceeded || std::abs(_lastDragPoint.X - _dragStart.X) > 3 ||
                        std::abs(_lastDragPoint.Y - _dragStart.Y) > 3;
        if (_pressedAction.kind == ::Notepads::Markdown::Native::ReadingActionKind::Task)
        {
            try
            {
                // A task gesture never updates reading selection or auto-scrolls.
                // Once it leaves the actual painted object it stays cancelled.
                auto hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTest(*_presented.frame, ControlToViewport(point).X - _padding,
                                                                                  ControlToViewport(point).Y - _padding);
                _dragExceeded = _dragExceeded || _readingPressedFrame.lock() != _presented.frame ||
                                _readingPressedTaskGeneration != _taskInteractionGeneration ||
                                _readingPressedPresentation != _presented.serial || !hit ||
                                HitReadingAction(*hit, point) != _pressedAction;
            }
            catch (...)
            {
                _dragExceeded = true;
            }
            return sameDrag() && !_dragExceeded;
        }
        try
        {
            auto action = CaptureReadingAction();
            auto hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTest(
                *_presented.frame, ControlToViewport(_lastDragPoint).X - _padding, ControlToViewport(_lastDragPoint).Y - _padding);
            if (hit)
            {
                auto anchor = _selectionAnchor;
                bool current = UpdateSelection(anchor, hit->position, true, SelectionMotion::Other, hit->trailingCaret);
                if (!sameDrag())
                    return false;
                if (!current || !CurrentReadingAction(action) || _selectionAnchor != anchor ||
                    _selectionActive != hit->position)
                {
                    StopReadingDrag();
                    return false;
                }
            }
        }
        catch (...)
        {
        }
        if (!sameDrag())
            return false;
        auto height = _presented.geometry.height;
        auto localDrag = ControlToViewport(_lastDragPoint);
        bool outside = localDrag.Y < 0 || localDrag.Y > height;
        if (outside)
        {
            if (!_readingDragTimer)
            {
                _readingDragTimer = DispatcherTimer();
                _readingDragTimer.Interval(std::chrono::milliseconds(40));
                _readingDragTickRevoker = _readingDragTimer.Tick(
                    auto_revoke,
                    [weak = get_weak()](auto const &, auto const &)
                    {
                        if (auto owner = weak.get())
                        {
                            if (!owner->_dragging || !owner->_readingPointerId || !owner->_previewScroll ||
                                !owner->ReadingAvailable())
                            {
                                owner->_readingDragTimer.Stop();
                                return;
                            }
                            auto drag = owner->_readingDragGeneration;
                            auto intent = owner->BeginImmediateScroll();
                            if (!owner->CurrentScrollIntent(intent) || drag != owner->_readingDragGeneration ||
                                !owner->ReadingAvailable())
                                return;
                            auto delta = owner->ControlToViewport(owner->_lastDragPoint).Y < 0 ? -20.0 : 20.0;
                            const auto target = std::clamp(intent.scroll.VerticalOffset() + delta,
                                                           0.0, intent.scroll.ScrollableHeight());
                            owner->CancelViewportRestore(target);
                            intent.scroll.ChangeView(nullptr, target, nullptr, true);
                            if (drag == owner->_readingDragGeneration)
                                owner->ContinueReadingDrag();
                        }
                    });
            }
            _readingDragTimer.Start();
        }
        else if (_readingDragTimer)
            _readingDragTimer.Stop();
        return true;
    }
    void MarkdownView::OnReadingPointerMoved(IInspectable const &,
                                             Windows::UI::Xaml::Input::PointerRoutedEventArgs const &args)
    {
        auto lifetime = get_strong();
        if (_diagramInspection)
            return;
        if (!_dragging)
        {
            if (args.Pointer().PointerDeviceType() == Windows::Devices::Input::PointerDeviceType::Mouse)
            {
                auto point = args.GetCurrentPoint(*this).Position();
                _codeCopyPointer = point;
                if (IsReadingPointerOnChrome(args.OriginalSource()))
                    _readingLinkPoint.reset();
                else
                    _readingLinkPoint = args.GetCurrentPoint(nullptr).Position();
                auto link = UpdateReadingLinkCursor();
                TrackReadingHover(point, link);
                UpdateCodeCopyButton();
            }
            return;
        }
        if (_readingPointerId != args.Pointer().PointerId())
            return;
        _lastDragPoint = args.GetCurrentPoint(*this).Position();
        ContinueReadingDrag();
        args.Handled(true);
    }
    void MarkdownView::OnReadingPointerReleased(IInspectable const &,
                                                Windows::UI::Xaml::Input::PointerRoutedEventArgs const &args)
    {
        auto lifetime = get_strong();
        if (!_dragging)
        {
            OnReadingPointerMoved(nullptr, args);
            return;
        }
        if (_readingPointerId != args.Pointer().PointerId())
            return;
        ReadingAction action{_presented.frame, _automationSnapshot, _presented.serial};
        auto drag = _readingDragGeneration;
        auto at = args.GetCurrentPoint(*this).Position();
        auto target = _pressedAction;
        auto pressedFrame = _readingPressedFrame.lock();
        auto pressedTaskGeneration = _readingPressedTaskGeneration;
        auto pressedPresentation = _readingPressedPresentation;
        _lastDragPoint = at;
        bool continued = ContinueReadingDrag();
        if (!_dragging || drag != _readingDragGeneration || _readingPointerId != args.Pointer().PointerId())
        {
            args.Handled(true);
            return;
        }
        auto clicked = !_dragExceeded;
        auto anchor = _selectionAnchor;
        auto active = _selectionActive;
        _dragging = _dragExceeded = false;
        _readingPointerId.reset();
        _pressedAction = {};
        _readingPressedFrame.reset();
        _readingPressedTaskGeneration = 0;
        if (_readingDragTimer)
            _readingDragTimer.Stop();
        ReleasePointerCapture(args.Pointer());
        if (args.Pointer().PointerDeviceType() == Windows::Devices::Input::PointerDeviceType::Mouse)
        {
            _codeCopyPointer = at;
            _readingLinkPoint.reset();
            try
            {
                // A captured release still reports the capture owner as its source.
                auto point = args.GetCurrentPoint(nullptr).Position();
                auto hits = VisualTreeHelper::FindElementsInHostCoordinates(point, *this).First();
                if (hits.HasCurrent() && !IsReadingPointerOnChrome(hits.Current()))
                    _readingLinkPoint = point;
            }
            catch (...)
            {
            }
        }
        UpdateReadingLinkCursor();
        UpdateCodeCopyButton();
        if (continued && clicked && target.kind != ::Notepads::Markdown::Native::ReadingActionKind::None &&
            drag == _readingDragGeneration && !_dragging && CurrentReadingAction(action) &&
            _selectionAnchor == anchor && _selectionActive == active && pressedPresentation == action.presentation)
        {
            try
            {
                auto hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTest(*_presented.frame, ControlToViewport(at).X - _padding,
                                                                                  ControlToViewport(at).Y - _padding);
                if (hit && HitReadingAction(*hit, at) == target)
                {
                    if (target.kind == ::Notepads::Markdown::Native::ReadingActionKind::Link)
                        InvokeReadingLink(*action.snapshot, target.index);
                    else if (target.kind == ::Notepads::Markdown::Native::ReadingActionKind::Disclosure)
                        SetReadingDisclosure(*action.snapshot, target.index);
                    else if (pressedFrame == action.frame && pressedTaskGeneration == _taskInteractionGeneration &&
                             ReadingTaskEnabled(*action.snapshot, target.index))
                        InvokeReadingTask(*action.snapshot, target.index);
                }
            }
            catch (...)
            {
            }
        }
        args.Handled(true);
    }
    void MarkdownView::OnReadingPointerCaptureLost(IInspectable const &,
                                                   Windows::UI::Xaml::Input::PointerRoutedEventArgs const &args)
    {
        auto lifetime = get_strong();
        if (_readingPointerId != args.Pointer().PointerId())
            return;
        _readingLinkPoint.reset();
        UpdateReadingLinkCursor();
        _dragging = _dragExceeded = false;
        _readingPointerId.reset();
        _pressedAction = {};
        _readingPressedFrame.reset();
        _readingPressedTaskGeneration = 0;
        if (_readingDragTimer)
            _readingDragTimer.Stop();
    }
    bool MarkdownView::MoveReadingCaretVertically(ReadingAction const &action, size_t next, bool extend)
    {
        auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || !CurrentReadingAction(action))
            return false;
        _pendingCaretLine = next;
        _pendingCaretFrameRequested = false;
        _pendingCaretExtend = extend;
        _pendingCaretText = action.snapshot->token;
        _pendingCaretLayout = _previewLayoutGeneration;
        auto const &line = action.frame->navigation->Line(next);
        const auto y = line.y + _padding;
        if (y < intent.scroll.VerticalOffset() ||
            y + line.height > intent.scroll.VerticalOffset() + intent.scroll.ViewportHeight())
        {
            const auto target = std::clamp(y - intent.scroll.ViewportHeight() * 0.4,
                                          0.0, std::max(0.0, intent.scroll.ScrollableHeight()));
            if (std::abs(target - intent.scroll.VerticalOffset()) * _viewportGeometry.scale > 0.5)
            {
                CancelViewportRestore(target);
                if (intent.scroll.ChangeView(nullptr, target, nullptr, true) ||
                    !CurrentScrollIntent(intent) || !CurrentReadingAction(action))
                    return true;
                _imageUserScrollY.reset();
            }
            else
            {
                // No ViewChanged event will finish the caret at this scroll bound.
                CancelViewportRestore();
            }
        }
        FinishPendingCaret();
        return true;
    }

    void MarkdownView::FinishPendingCaret()
    {
        auto lifetime = get_strong();
        if (!_pendingCaretLine || !ReadingAvailable())
            return;
        if (_pendingCaretText != _automationSnapshot->token || _pendingCaretLayout != _previewLayoutGeneration)
        {
            _pendingCaretLine.reset();
            return;
        }
        auto action = CaptureReadingAction();
        auto const &navigation = *action.frame->navigation;
        if (*_pendingCaretLine >= navigation.LineCount())
        {
            _pendingCaretLine.reset();
            return;
        }
        auto line = navigation.Line(*_pendingCaretLine);
        std::optional<::Notepads::Markdown::Native::ReadingHit> hit;
        try
        {
            hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTestLine(
                *action.frame, *_pendingCaretLine, _preferredCaretX);
            if (hit && (hit->position < line.text.start || hit->position > line.text.end))
                throw ::Notepads::Markdown::Native::ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE};
        }
        catch (...)
        {
            // A materialized row with no legal caret is an actual hit-test
            // failure, not a reason to repeatedly regenerate the same Frame.
            _renderingError = L"Keyboard line hit failed: " + hresult_error(to_hresult()).message();
            InvalidateVerticalNavigation();
            return;
        }
        if (hit)
        {
            auto extend = _pendingCaretExtend;
            _pendingCaretLine.reset();
            UpdateSelection(extend ? _selectionAnchor : hit->position, hit->position, true,
                            SelectionMotion::Vertical, line.wrappedEnd && hit->position == line.text.end);
        }
        else if (!_pendingCaretFrameRequested)
        {
            // At most one explicit materialization request per keyboard action.
            // Scroll events may still deliver the requested same-layout Frame;
            // its absence never falls back to a different visible line.
            _pendingCaretFrameRequested = true;
            StartPreviewFrame();
        }
    }
    void MarkdownView::OnReadingKeyDown(IInspectable const &, Windows::UI::Xaml::Input::KeyRoutedEventArgs const &args)
    {
        auto lifetime = get_strong();
        if (!ReadingAvailable())
            return;
        using Key = Windows::System::VirtualKey;
        auto key = args.Key();
        const bool copyFocused = _codeCopyButton && _codeCopyButton.FocusState() != FocusState::Unfocused;
        if (copyFocused && key != Key::Tab)
            return;
        using Unit = Windows::UI::Xaml::Automation::Text::TextUnit;
        auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
        auto down = [&](Key key)
        {
            return window && (window.GetKeyState(key) & Windows::UI::Core::CoreVirtualKeyStates::Down) !=
                   Windows::UI::Core::CoreVirtualKeyStates::None;
        };
        // Alt combinations belong to the host, including Alt+Ctrl variants.
        if (args.KeyStatus().IsMenuKeyDown || down(Key::Menu))
            return;
        bool control = down(Key::Control), shift = down(Key::Shift);
        if (copyFocused && control)
            return;
        try
        {
            if (_pendingDiagramInspection && key != Key::Enter)
                _pendingDiagramInspection.reset();
            if (_diagramInspection &&
                (key == Key::PageUp || key == Key::PageDown || key == Key::End ||
                 (control && (key == Key::Home || key == Key::Left || key == Key::Right ||
                              key == Key::Up || key == Key::Down))))
            {
                auto snapshot = _automationSnapshot;
                _diagramInspection.reset();
                DrawPreview();
                if (!ReadingAvailable() || _automationSnapshot != snapshot)
                    return;
            }
            if (_diagramInspection && !control && ApplyDiagramInspectionKey(key))
            {
                args.Handled(true);
                return;
            }
            if (control && key == Key::A)
            {
                auto bounds = _automationSnapshot->Bounds();
                UpdateSelection(bounds.start, bounds.end);
                args.Handled(true);
                return;
            }
            if (control && key == Key::C)
            {
                (void)CopySelection();
                args.Handled(true);
                return;
            }
            if (key == Key::Escape)
            {
                _focusedAction = {};
                InvalidateVerticalNavigation();
                StopReadingDrag();
                DrawPreview();
                args.Handled(true);
                return;
            }
            using Kind = ::Notepads::Markdown::Native::ReadingActionKind;
            using Child = ::Notepads::Markdown::Native::ReadingChildKind;
            if (_focusedAction.kind == Kind::Diagram && key == Key::Enter && !control && !shift)
            {
                args.Handled(true);
                if (!args.KeyStatus().WasKeyDown)
                    InvokeReadingDiagram(*_automationSnapshot, _focusedAction.index);
                return;
            }
            if (_focusedAction.kind == Kind::Disclosure && !control && !shift &&
                (key == Key::Enter || key == Key::Space || key == Key::Left || key == Key::Right))
            {
                args.Handled(true);
                if (!args.KeyStatus().WasKeyDown)
                {
                    std::optional<bool> expanded;
                    if (key == Key::Left || key == Key::Right)
                        expanded = key == Key::Right;
                    SetReadingDisclosure(*_automationSnapshot, _focusedAction.index, expanded);
                }
                return;
            }
            if (key == Key::Enter && _focusedAction.kind == Kind::Link)
            {
                auto snapshot = _automationSnapshot;
                auto index = _focusedAction.index;
                args.Handled(true);
                InvokeReadingLink(*snapshot, index);
                return;
            }
            if (key == Key::Space && !control && _focusedAction.kind == Kind::Task)
            {
                auto snapshot = _automationSnapshot;
                auto index = _focusedAction.index;
                args.Handled(true);
                if (!args.KeyStatus().WasKeyDown && ReadingTaskEnabled(*snapshot, index))
                    InvokeReadingTask(*snapshot, index);
                return;
            }
            auto action = CaptureReadingAction();
            auto const &navigation = *action.frame->navigation;
            if (key == Key::Tab && !control)
            {
                if (copyFocused && shift)
                {
                    if (Focus(FocusState::Keyboard))
                        args.Handled(true);
                    return;
                }
                if (!copyFocused && !shift && _focusedAction.kind == Kind::None &&
                    FocusState() == FocusState::Keyboard && _selectionAnchor == _selectionActive)
                {
                    UpdateCodeCopyButton(true);
                    if (auto target = _codeCopyTarget; target && _codeCopyButton &&
                        _selectionActive >= target->start && _selectionActive <= target->end &&
                        _codeCopyButton.Visibility() == Visibility::Visible)
                    {
                        auto button = _codeCopyButton;
                        if (button.Focus(FocusState::Keyboard))
                        {
                            args.Handled(true);
                            return;
                        }
                        if (!button.IsLoaded())
                        {
                            // Tab can retarget the button from the mouse's block to the caret's block.
                            // A freshly attached control needs to finish loading before taking focus.
                            _codeCopyFocusLoaded = button.Loaded(auto_revoke,
                                [weak = get_weak(), target = *target](auto const &sender, auto const &)
                                {
                                    if (auto owner = weak.get(); owner && sender == owner->_codeCopyButton)
                                    {
                                        owner->_codeCopyFocusLoaded.revoke();
                                        if (owner->_codeCopyTarget == target && owner->CodeCopyViewportCurrent() &&
                                            owner->FocusState() == FocusState::Keyboard &&
                                            owner->_focusedAction.kind == Kind::None &&
                                            owner->_selectionAnchor == owner->_selectionActive &&
                                            owner->_selectionActive >= target.start && owner->_selectionActive <= target.end &&
                                            owner->_codeCopyButton.Visibility() == Visibility::Visible)
                                            owner->_codeCopyButton.Focus(FocusState::Keyboard);
                                    }
                                });
                            args.Handled(true);
                            return;
                        }
                    }
                }
                auto const &descriptors = *action.snapshot->descriptors;
                std::optional<::Notepads::Markdown::Native::ReadingActionTarget> next, previous;
                bool haveFocus = _focusedAction.kind != Kind::None, seenFocus{};
                descriptors.VisitChildren([&](Child child, size_t index)
                {
                    if ((child != Child::Link && child != Child::Disclosure && child != Child::Object) ||
                        index > UINT32_MAX || !ReadingChildVisible(*action.snapshot, descriptors.Child(child, index)))
                        return true;
                    auto kind = child == Child::Link ? Kind::Link : child == Child::Disclosure ? Kind::Disclosure :
                        descriptors.objects[index].kind == ::Notepads::Markdown::Native::ReadingObjectKind::Diagram
                            ? Kind::Diagram : Kind::Task;
                    if (child == Child::Object &&
                        !(kind == Kind::Diagram ? ReadingDiagramEnabled(*action.snapshot, index) :
                                                  ReadingTaskEnabled(*action.snapshot, index)))
                        return true;
                    ::Notepads::Markdown::Native::ReadingActionTarget candidate{kind,
                                                                         static_cast<uint32_t>(index)};
                    if (!haveFocus)
                    {
                        next = candidate;
                        if (!shift)
                            return false;
                    }
                    else if (candidate == _focusedAction)
                    {
                        seenFocus = true;
                        if (shift)
                        {
                            next = previous;
                            return false;
                        }
                    }
                    else if (seenFocus)
                    {
                        next = candidate;
                        return false;
                    }
                    previous = candidate;
                    return true;
                });
                if (!next)
                {
                    _focusedAction = {};
                    DrawPreview();
                    return;
                }
                FocusReadingAction(*action.snapshot, *next);
                args.Handled(true);
                return;
            }
            uint32_t target = _selectionActive;
            bool trailingCaret{};
            int32_t moved{};
            if (key == Key::Left || key == Key::Right)
            {
                if (!shift && !control && _selectionAnchor != _selectionActive)
                    target = key == Key::Left ? std::min(_selectionAnchor, _selectionActive)
                                              : std::max(_selectionAnchor, _selectionActive);
                else
                    target = navigation.Move(_selectionActive, control ? Unit::Word : Unit::Character,
                                             key == Key::Left ? -1 : 1, moved);
                InvalidateVerticalNavigation();
            }
            else if (key == Key::Home || key == Key::End)
            {
                if (control)
                    target = navigation.Move(_selectionActive, Unit::Document, key == Key::Home ? -1 : 1, moved);
                else
                {
                    if (!navigation.LineCount())
                        return;
                    auto const &line = navigation.Line(navigation.LineAt(_selectionActive, _readingCaretTrailing));
                    target = key == Key::Home ? line.text.start : line.text.end;
                    trailingCaret = key == Key::End && line.wrappedEnd;
                }
                InvalidateVerticalNavigation();
            }
            else if (key == Key::Up || key == Key::Down || key == Key::PageUp || key == Key::PageDown)
            {
                if (!navigation.LineCount())
                    return;
                auto current = _pendingCaretLine.value_or(navigation.LineAt(_selectionActive, _readingCaretTrailing));
                if (!_hasPreferredCaretX)
                {
                    // Ask for the caret itself: the following logical unit can
                    // be an unpainted paragraph separator at the end of a line.
                    auto rectangles = ::Notepads::Markdown::Native::NativeDocumentRenderer::Rectangles(
                        *action.frame, _selectionActive, _selectionActive, _readingCaretTrailing);
                    _preferredCaretX = rectangles.empty() ? navigation.Line(current).x : rectangles.front().x;
                    _hasPreferredCaretX = true;
                }
                auto page = key == Key::PageUp || key == Key::PageDown;
                auto distance = page ? (_previewScroll ? _previewScroll.ViewportHeight() : ActualHeight()) : 0;
                auto next = navigation.VisualLine(current, key == Key::Down || key == Key::PageDown, distance,
                                                  _preferredCaretX);
                if (MoveReadingCaretVertically(action, next, shift))
                    args.Handled(true);
                return;
            }
            else
                return;
            MoveReadingCaret(target, shift, trailingCaret);
            args.Handled(true);
        }
        catch (...)
        {
        }
    }
    void MarkdownView::OnReadingFocusChanged(IInspectable const &, RoutedEventArgs const &)
    {
        auto lifetime = get_strong();
        _readingFocused = FocusState() != Windows::UI::Xaml::FocusState::Unfocused;
        if (!_readingFocused)
        {
            _diagramInspection.reset();
            _pendingDiagramInspection.reset();
            _focusedAction = {};
            StopReadingDrag();
        }
        if (_documentAccepted)
            DrawPreview();
    }

}
