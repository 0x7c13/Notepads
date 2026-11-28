// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Control/MarkdownOutline.h"
#include "Reading/NativeReadingOutline.h"

namespace winrt::Notepads::Markdown::implementation
{
    Markdown::MarkdownOutline MarkdownView::GetOutline() const
    {
        CheckUiThread();
        if (!ReadingAvailable() || !_automationSnapshot->descriptors || !_automationSnapshot->descriptors->outline)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto const snapshot = _automationSnapshot;
        CheckLiveRange(*snapshot);
        return make<implementation::MarkdownOutline>(snapshot->document, snapshot->revision, snapshot->token,
                                                       snapshot->descriptors->outline, Dispatcher(), snapshot->section);
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    com_array<double> MarkdownView::StartupHeadingGeometryForDiagnostics(hstring const &title) const
    {
        CheckUiThread();
        if (!ReadingAvailable() || !_automationSnapshot->descriptors || !_automationSnapshot->descriptors->outline)
            return {};
        auto const snapshot = _automationSnapshot;
        auto const &navigation = ReadingNavigation(*snapshot);
        auto const &frame = *_presented.frame;
        std::vector<double> samples;
        for (auto const &heading : snapshot->descriptors->outline->Headings())
        {
            if (std::wstring_view{heading.title} != std::wstring_view{title})
                continue;
            auto rectangles = ::Notepads::Markdown::Native::NativeDocumentRenderer::Rectangles(
                frame, heading.text.start, heading.text.end);
            double top = _presented.geometry.height, bottom = 0;
            if (!rectangles.empty())
            {
                top = rectangles.front().y + _padding;
                bottom = top;
                for (auto const &rect : rectangles)
                {
                    top = std::min(top, static_cast<double>(rect.y) + _padding);
                    bottom = std::max(bottom, static_cast<double>(rect.y) + _padding + rect.height);
                }
            }
            // Match the exact source-mapped occurrence in the observer, even
            // when an earlier heading has the same displayed title.
            samples.insert(samples.end(), {frame.viewportY, _presented.geometry.height,
                navigation.RevealBlockY(heading.node) + _padding - frame.viewportY, top, bottom,
                static_cast<double>(rectangles.size()), static_cast<double>(heading.text.start),
                static_cast<double>(heading.text.end)});
        }
        return {samples.begin(), samples.end()};
    }
#endif

    void MarkdownView::NavigateToHeading(guid document, uint64_t revision, uint64_t textGeneration,
                                         uint32_t headingId)
    {
        CheckUiThread();
        if (!ReadingAvailable())
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto const snapshot = _automationSnapshot;
        CheckLiveRange(*snapshot);
        if (snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration ||
            !snapshot->descriptors || !snapshot->descriptors->outline)
            throw hresult_invalid_argument();
        auto const &outline = *snapshot->descriptors->outline;
        auto headings = outline.Headings();
        if (!headingId || headingId > headings.size() || outline.TextGeneration() != textGeneration ||
            (snapshot->section && !snapshot->section->ContainsHeading(headingId - 1)))
            throw hresult_invalid_argument();
        auto const &heading = headings[headingId - 1];
        RequestReadingReveal(snapshot->token, heading.text, heading.node, true, false, true);
    }

    Markdown::MarkdownFragmentResult MarkdownView::NavigateToFragment(
        guid document, uint64_t revision, uint64_t textGeneration, hstring const &destination)
    {
        using Result = Markdown::MarkdownFragmentResult;
        CheckUiThread();
        if (!ReadingAvailable())
            return Result::Pending;
        auto const snapshot = _automationSnapshot;
        CheckLiveRange(*snapshot);
        if (snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration ||
            !snapshot->descriptors || !snapshot->descriptors->outline)
            throw hresult_invalid_argument();
        auto const &outline = *snapshot->descriptors->outline;
        auto decoded = ::Notepads::Markdown::Native::DecodeReadingFragment(destination, outline.Memory());
        if (!decoded)
            return Result::NotFound;
        _pendingReadingReveal.reset();
        auto action = CaptureReadingAction();
        auto target = 0.0;
        if (!decoded->empty())
        {
            std::wstring_view key{decoded->data(), decoded->size()};
            auto custom = outline.FindCustomAnchor(key);
            auto heading = outline.FindAnchor(key);
            if (!heading && !custom)
                return Result::NotFound;
            bool useCustom = custom && (!heading || (!custom->legacy && custom->position <= outline.Headings()[*heading].text.start));
            auto node = useCustom ? custom->node : outline.Headings()[*heading].node;
            auto position = useCustom ? custom->position : outline.Headings()[*heading].text.start;
            if (snapshot->section && (useCustom ? position < snapshot->section->text.start || position >= snapshot->section->text.end
                                                : !snapshot->section->ContainsHeading(*heading)))
                return Result::NotFound;
            if (ExpandReadingDisclosures({position, position}, node))
                return Result::Pending;
            auto const &navigation = ReadingNavigation(*snapshot);
            target = (node ? navigation.RevealBlockY(node) : navigation.RevealY(position)) + _padding;
        }
        return TryScrollToReadingTarget(action, target);
    }

    Markdown::MarkdownFragmentResult MarkdownView::NavigateToPreviewPosition(
        guid document, uint64_t revision, uint64_t textGeneration, uint64_t position)
    {
        CheckUiThread();
        if (!ReadingAvailable())
            return Markdown::MarkdownFragmentResult::Pending;
        auto action = CaptureReadingAction();
        auto const &snapshot = *action.snapshot;
        if (snapshot.document != document || snapshot.revision != revision || snapshot.token != textGeneration ||
            position > snapshot.main->Length())
            throw hresult_invalid_argument();
        auto const offset = static_cast<uint32_t>(position);
        snapshot.main->ValidateRange(offset, 0);
        if (!snapshot.Bounds().Contains(offset, offset))
            return Markdown::MarkdownFragmentResult::NotFound;
        _pendingReadingReveal.reset();
        if (ExpandReadingDisclosures({offset, offset}))
            return Markdown::MarkdownFragmentResult::Pending;
        auto const target = ReadingNavigation(snapshot).RevealY(offset) + _padding;
        return TryScrollToReadingTarget(action, target);
    }

    Markdown::MarkdownFragmentResult MarkdownView::NavigateToHeadingNode(
        guid document, uint64_t revision, uint64_t textGeneration, uint32_t node)
    {
        return NavigateToHeadingTarget(document, revision, textGeneration, node, {});
    }

    Markdown::MarkdownFragmentResult MarkdownView::NavigateToHeadingTitle(
        guid document, uint64_t revision, uint64_t textGeneration, hstring const &title)
    {
        return NavigateToHeadingTarget(document, revision, textGeneration, 0, title);
    }

    Markdown::MarkdownFragmentResult MarkdownView::NavigateToHeadingTarget(
        guid document, uint64_t revision, uint64_t textGeneration, uint32_t node, std::wstring_view title)
    {
        using Result = Markdown::MarkdownFragmentResult;
        CheckUiThread();
        if (!ReadingAvailable())
            return Result::Pending;
        auto const snapshot = _automationSnapshot;
        CheckLiveRange(*snapshot);
        if ((!node && title.empty()) || snapshot->document != document || snapshot->revision != revision ||
            snapshot->token != textGeneration || !snapshot->descriptors || !snapshot->descriptors->outline)
            throw hresult_invalid_argument();
        auto const &outline = *snapshot->descriptors->outline;
        auto const heading = node ? outline.FindNode(node) : outline.FindTitle(title);
        if (!heading || (snapshot->section && !snapshot->section->ContainsHeading(heading - outline.Headings().data())))
            return Result::NotFound;
        _pendingReadingReveal.reset();
        if (ExpandReadingDisclosures(heading->text, heading->node))
            return Result::Pending;
        auto action = CaptureReadingAction();
        auto target = ReadingNavigation(*snapshot).RevealBlockY(heading->node) + _padding;
        return TryScrollToReadingTarget(action, target);
    }

    void MarkdownView::RequestReadingReveal(uint64_t textGeneration, ::Notepads::Markdown::Native::ReadingSpan span,
                                             uint32_t node, bool alignTop, bool externalScroll, bool expandDisclosures)
    {
        _pendingDiagramInspection.reset();
        _pendingReadingReveal = PendingReadingReveal{++_readingRevealSerial, textGeneration, span, node, alignTop, externalScroll};
        if (!expandDisclosures || !ExpandReadingDisclosures(span, node))
            TryApplyPendingReadingReveal();
    }

    void MarkdownView::TryApplyPendingReadingReveal()
    {
        if (!_pendingReadingReveal || _applyingReadingReveal || !ReadingAvailable() || !_previewScroll)
            return;
        auto lifetime = get_strong();
        auto pending = *_pendingReadingReveal;
        auto action = CaptureReadingAction();
        if (pending.textGeneration != action.snapshot->token)
        {
            _pendingReadingReveal.reset();
            return;
        }
        if (!ScrollExtentCurrent(CaptureScrollIntent(), action.frame->totalHeight))
            return;
        auto const &navigation = *action.frame->navigation;
        double target = _padding;
        if (pending.node)
            target += navigation.RevealBlockY(pending.node);
        else if (pending.alignTop)
            target += navigation.RevealY(pending.span.start);
        else
            target += navigation.RevealBottomY(pending.span) - _previewScroll.ViewportHeight();
        struct ResetOnExit
        {
            bool &active;
            ~ResetOnExit() { active = false; }
        };
        _applyingReadingReveal = true;
        ResetOnExit reset{_applyingReadingReveal};
        const auto result = TryScrollToReadingTarget(action, target, pending.externalScroll);
        if (!_pendingReadingReveal)
            return;
        if (_pendingReadingReveal->serial == pending.serial)
        {
            if (result == Markdown::MarkdownFragmentResult::Applied)
            {
                _pendingReadingReveal.reset();
                return;
            }
            if (pending.scrollStartAnnounced || !_pendingReadingReveal->scrollStartAnnounced)
                return;
        }
        // A scroll observer can interrupt this attempt or queue a replacement without
        // causing another layout or scroll event. Give the pending request one retry.
        auto next = _pendingReadingReveal->serial;
        (void)Dispatcher().TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal, [weak = get_weak(), next]
        {
            if (auto owner = weak.get(); owner && owner->_pendingReadingReveal &&
                owner->_pendingReadingReveal->serial == next)
                owner->TryApplyPendingReadingReveal();
        });
    }

    Markdown::MarkdownFragmentResult MarkdownView::TryScrollToReadingTarget(ReadingAction const &action, double target,
                                                                         bool externalScroll)
    {
        using Result = Markdown::MarkdownFragmentResult;
        auto revealSerial = _readingRevealSerial;
        auto intent = externalScroll ? CaptureScrollIntent() : BeginImmediateScroll();
        auto current = [&]
        {
            return _readingRevealSerial == revealSerial && (!externalScroll || _pendingReadingReveal) &&
                   CurrentScrollIntent(intent) && CurrentReadingAction(action) &&
                   ScrollExtentCurrent(intent, action.frame->totalHeight);
        };
        if (!current())
            return Result::Pending;
        auto y = std::clamp(target, 0.0, intent.scroll.ScrollableHeight());
        auto distance = std::abs(y - intent.scroll.VerticalOffset()) * _viewportGeometry.scale;
        if (externalScroll ? distance == 0.0 : distance <= 0.5)
        {
            CancelViewportRestore();
            return Result::Applied;
        }
        if (externalScroll)
        {
            if (!BeginVerticalReadingScroll(intent.scroll, revealSerial))
                return Result::Pending;
            intent = CaptureScrollIntent();
            if (!current())
                return Result::Pending;
            y = std::clamp(target, 0.0, intent.scroll.ScrollableHeight());
        }
        CancelViewportRestore(y);
        const bool accepted = intent.scroll.ChangeView(nullptr, y, nullptr, true);
        if (!CurrentScrollIntent(intent) || _readingRevealSerial != revealSerial)
            return Result::Pending;
        if (accepted)
            return Result::Applied;
        _imageUserScrollY.reset();
        return Result::Pending;
    }
}
