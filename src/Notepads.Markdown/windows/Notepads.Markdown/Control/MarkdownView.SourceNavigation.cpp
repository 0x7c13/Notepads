// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Diagnostics/NativeStartupTrace.h"
#endif
#include "Control/MarkdownMappedPosition.h"
#include "Control/MarkdownMappedSpan.h"
#include "Control/MarkdownViewportAnchor.h"
#include "Reading/NativeSourceNavigation.h"

namespace winrt::Notepads::Markdown::implementation
{
    Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedPosition> MarkdownView::MapSourcePositionAsync(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t offset,
        Markdown::MarkdownPositionAffinity affinity)
    {
        return MapPositionAsync(document, revision, textGeneration, offset, affinity, true);
    }
    Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedPosition> MarkdownView::MapPreviewPositionAsync(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t offset,
        Markdown::MarkdownPositionAffinity affinity)
    {
        return MapPositionAsync(document, revision, textGeneration, offset, affinity, false);
    }
    Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedSpan> MarkdownView::MapSourceSpanAsync(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t end,
        Markdown::MarkdownPositionAffinity affinity)
    {
        return MapSpanAsync(document, revision, textGeneration, start, end, affinity, true);
    }
    Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedSpan> MarkdownView::MapPreviewSpanAsync(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t end,
        Markdown::MarkdownPositionAffinity affinity)
    {
        return MapSpanAsync(document, revision, textGeneration, start, end, affinity, false);
    }
    Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedSpan> MarkdownView::MapSpanAsync(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t start, uint64_t end,
        Markdown::MarkdownPositionAffinity affinity, bool fromSource)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        CheckUiThread();
        if (start > end ||
            (affinity != Markdown::MarkdownPositionAffinity::Before && affinity != Markdown::MarkdownPositionAffinity::After))
            throw hresult_invalid_argument();
        if (!ReadingAvailable())
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto snapshot = _automationSnapshot;
        if (snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration)
            throw hresult_invalid_argument();
        auto owner = get_weak();
        auto dispatcher = Dispatcher();
        auto flag = std::make_shared<::Notepads::Markdown::Native::PreviewCancellation>(snapshot->main->ModelOwner().Get());
        auto cancellation = co_await get_cancellation_token();
        cancellation.callback([flag] { flag->Cancel(); });
        struct CallbackGuard
        {
            decltype(cancellation) token;
            ~CallbackGuard() noexcept
            {
                token.callback(nullptr);
            }
        } guard{cancellation};
        co_await resume_background();
        if (cancellation())
            throw hresult_canceled();
        auto result = ::Notepads::Markdown::Native::MapReadingSpan(*snapshot->main, flag->Get(), fromSource, start, end,
                                                            affinity == Markdown::MarkdownPositionAffinity::After);
        struct MappingCompletion
        {
            ::Notepads::Markdown::Native::ModuleLifetime module;
            Markdown::MarkdownMappedSpan value{nullptr};
            std::atomic<int32_t> status{E_UNEXPECTED};
        };
        auto completed = std::make_shared<MappingCompletion>();
        auto dispatched = co_await resume_agile(dispatcher.TryRunAsync(
            Windows::UI::Core::CoreDispatcherPriority::Normal,
            [completed, owner, snapshot, document, revision, textGeneration, result, affinity]
            {
                try
                {
                    auto control = owner.get();
                    if (!control || !control->ReadingAvailable())
                        throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
                    control->CheckLiveRange(*snapshot);
                    auto point = [&](auto const &value)
                    {
                        return make<implementation::MarkdownMappedPosition>(
                            document, revision, textGeneration, value.sourceByteOffset, value.previewUtf16Offset,
                            affinity, static_cast<Markdown::MarkdownCorrespondence>(value.correspondence), value.effects);
                    };
                    completed->value = make<implementation::MarkdownMappedSpan>(point(result.start), point(result.end), result.discontinuous);
                    completed->status.store(S_OK, std::memory_order_release);
                }
                catch (...)
                {
                    completed->status.store(static_cast<int32_t>(to_hresult()), std::memory_order_release);
                }
            }));
        if (cancellation())
            throw hresult_canceled();
        if (!dispatched)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        check_hresult(completed->status.load(std::memory_order_acquire));
        co_return completed->value;
    }
    Windows::Foundation::IAsyncOperation<Markdown::MarkdownMappedPosition> MarkdownView::MapPositionAsync(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t offset,
        Markdown::MarkdownPositionAffinity affinity, bool fromSource)
    {
        ::Notepads::Markdown::Native::ModuleLifetime module;
        CheckUiThread();
        if (affinity != Markdown::MarkdownPositionAffinity::Before && affinity != Markdown::MarkdownPositionAffinity::After)
            throw hresult_invalid_argument();
        if (!ReadingAvailable())
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto snapshot = _automationSnapshot;
        if (snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration)
            throw hresult_invalid_argument();
        auto owner = get_weak();
        auto dispatcher = Dispatcher();
        auto flag = std::make_shared<::Notepads::Markdown::Native::PreviewCancellation>(snapshot->main->ModelOwner().Get());
        auto cancellation = co_await get_cancellation_token();
        cancellation.callback([flag] { flag->Cancel(); });
        struct CallbackGuard
        {
            decltype(cancellation) token;
            ~CallbackGuard() noexcept
            {
                token.callback(nullptr);
            }
        } guard{cancellation};
        co_await resume_background();
        if (cancellation())
            throw hresult_canceled();
        auto result = ::Notepads::Markdown::Native::MapReadingPosition(*snapshot->main, flag->Get(), fromSource, offset,
                                                                affinity == Markdown::MarkdownPositionAffinity::After);
        struct MappingCompletion
        {
            // The queued delegate can outlive a canceled coroutine. Its own
            // module lease remains until after its immutable result is freed.
            ::Notepads::Markdown::Native::ModuleLifetime module;
            Markdown::MarkdownMappedPosition value{nullptr};
            std::atomic<int32_t> status{E_UNEXPECTED};
        };
        auto completed = std::make_shared<MappingCompletion>();
        auto dispatched = co_await resume_agile(dispatcher.TryRunAsync(
            Windows::UI::Core::CoreDispatcherPriority::Normal,
            [completed, owner, snapshot, document, revision, textGeneration, result, affinity]
            {
                // Own every queued value; no coroutine or stack references.
                // Only this callback reads XAML/control state.
                try
                {
                    auto control = owner.get();
                    if (!control || !control->ReadingAvailable())
                        throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
                    control->CheckLiveRange(*snapshot);
                    completed->value = make<implementation::MarkdownMappedPosition>(
                        document, revision, textGeneration, result.sourceByteOffset, result.previewUtf16Offset,
                        affinity, static_cast<Markdown::MarkdownCorrespondence>(result.correspondence), result.effects);
                    completed->status.store(S_OK, std::memory_order_release);
                }
                catch (...)
                {
                    completed->status.store(static_cast<int32_t>(to_hresult()), std::memory_order_release);
                }
            }));
        if (cancellation())
            throw hresult_canceled();
        if (!dispatched)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        check_hresult(completed->status.load(std::memory_order_acquire));
        co_return completed->value;
    }
    void MarkdownView::RevealRange(guid document, uint64_t revision, uint64_t textGeneration, uint64_t start,
                                   uint64_t length, bool alignTop, bool expandDisclosures)
    {
        CheckUiThread();
        if (!ReadingAvailable() || document != _automationSnapshot->document ||
            revision != _automationSnapshot->revision || textGeneration != _automationSnapshot->token ||
            start > UINT32_MAX || length > UINT32_MAX - start)
            throw hresult_invalid_argument();
        RevealReadingRange(*_automationSnapshot, static_cast<uint32_t>(start), static_cast<uint32_t>(start + length),
                           alignTop, false, expandDisclosures);
    }
    Markdown::MarkdownViewportAnchor MarkdownView::GetViewportAnchor() const
    {
        CheckUiThread();
        auto unavailable = [] { return make<implementation::MarkdownViewportAnchor>(false, guid{}, 0, 0, 0, 0, 0.0, 0, 0, 0.0); };
        if (!ReadingAvailable())
            return unavailable();
        auto snapshot = _automationSnapshot;
        try
        {
            auto const &navigation = ReadingNavigation(*snapshot);
            if (!navigation.LineCount())
                return unavailable();
            auto const &frame = *_presented.frame;
            auto const &geometry = _presented.geometry;
            auto y = frame.viewportY;
            auto viewportTop = y - _padding;
            auto span = navigation.ContinuousFollowSpan(viewportTop);
            auto top = DiagramInspectionReadingRect(*snapshot);
            if (!top)
            {
                for (auto index : navigation.VisibleLines(viewportTop, viewportTop + geometry.height))
                {
                    auto const &line = navigation.Line(index);
                    if (top && navigation.VisibleLineTop(line) > viewportTop + top->y - geometry.y + 0.25)
                        break;
                    if (navigation.VisibleLineBottom(line) <= viewportTop)
                        continue;
                    for (auto rect : ::Notepads::Markdown::Native::NativeDocumentRenderer::LineRectangles(frame, line))
                    {
                        rect = ClipReadingRectangle(rect);
                        if (rect.width > 0 && rect.height > 0 &&
                            (!top || rect.y < top->y || (rect.y == top->y && rect.x < top->x)))
                            top = rect;
                    }
                }
            }
            // A drawn frame can contain only non-text blocks such as thematic breaks.
            // Retain its scroll identity so source follow can move beyond that frame.
            auto start = top ? top->start : span.start;
            auto const &line = navigation.Line(navigation.LineAt(start));
            return make<implementation::MarkdownViewportAnchor>(true, snapshot->document, snapshot->revision,
                                                                snapshot->token, navigation.Key().layout_generation,
                                                                start, y - line.y, span.start, span.end, span.progress);
        }
        catch (hresult_error const &error)
        {
            if (error.code() == UIA_E_ELEMENTNOTAVAILABLE)
                return unavailable();
            throw;
        }
    }
    event_token MarkdownView::ViewportChanged(
        Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownViewportAnchor> const &handler)
    {
        return _viewportChanged.add(handler);
    }
    void MarkdownView::ViewportChanged(event_token const &token) noexcept
    {
        _viewportChanged.remove(token);
    }
    void MarkdownView::ShowInactiveCaretMarker(bool value)
    {
        CheckUiThread();
        if (_showInactiveCaretMarker == value)
            return;
        _showInactiveCaretMarker = value;
        if (_surfaceReady)
            DrawPreview();
    }

    void MarkdownView::RaiseViewportChanged()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiRaiseViewport, UiTrace::Phase::UiRaiseViewportEnd, _previewLayoutGeneration};
#endif
        try
        {
            auto anchor = GetViewportAnchor();
            if (anchor.IsAvailable())
            {
                if (_cacheDocumentOnUnload)
                {
                    const auto y = _presented.frame->viewportY;
                    const auto maximum = _previewScroll.ScrollableHeight();
                    const auto tolerance = std::min(0.5 / std::max(1.0, _viewportGeometry.scale), maximum * 0.5);
                    _lastViewport = CachedViewport{static_cast<uint32_t>(anchor.PreviewUtf16Offset()),
                        anchor.OffsetWithinLine(), y, maximum > 0 && y >= maximum - tolerance};
                }
                _viewportChanged(*this, anchor);
            }
        }
        catch (...)
        {
            // A host callback never invalidates a successfully published Frame.
        }
    }
}
