// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Control/MarkdownView.ReadingState.h"
#include "Control/MarkdownSelection.h"
#include "Control/MarkdownLinkRequestedEventArgs.h"
#include "Automation/MarkdownViewAutomationPeer.h"
#include "Automation/ReadingObjectProvider.h"
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.UI.Xaml.Automation.h>
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
    using ::Notepads::Markdown::Native::ReadingChildKind;
    using ::Notepads::Markdown::Native::ReadingSpan;

    hstring TextSnapshot::Export(uint32_t start, uint32_t length) const
    {
        auto view = View();
        auto bounds = Bounds();
        if (start < bounds.start || start > bounds.end || length > bounds.end - start)
            throw hresult_invalid_argument();
        if (main)
        {
            auto funded = main->CopyText(start, length);
            return hstring{std::wstring_view(funded.data(), funded.size())};
        }
        return hstring{view.substr(start, length)};
    }
    hstring MarkdownView::DisplayedText() const
    {
        CheckUiThread();
        if (_documentAccepted && _automationSnapshot && _automationSnapshot->main)
        {
            auto bounds = _automationSnapshot->Bounds();
            return _automationSnapshot->Export(bounds.start, bounds.end - bounds.start);
        }
        return {};
    }
    bool MarkdownView::ReadingAvailable() const noexcept
    {
        return _surfaceReady && _presented.frame && CurrentPreviewGeometry(*_presented.frame) &&
               _documentAccepted && _loaded && _readingSource && _automationSnapshot && _automationSnapshot->main &&
               _automationSnapshot->main->SourceGeneration() == _previewSourceGeneration &&
               _automationSnapshot->token == _readingSource->textGeneration;
    }
    bool MarkdownView::SetViewportBetweenPositions(guid document, uint64_t revision, uint64_t textGeneration,
                                                    uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                                    double progress)
    {
        return SetFollowViewportBetweenPositions(document, revision, textGeneration, layoutGeneration, start, end,
                                                 progress, 0, 0);
    }

    bool MarkdownView::SetFollowViewportBetweenPositions(guid document, uint64_t revision, uint64_t textGeneration,
                                                          uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                                          double progress, double topBlend, double bottomBlend)
    {
        return ApplyFollowViewport(document, revision, textGeneration, layoutGeneration, start, end,
                                   progress, topBlend, bottomBlend, false);
    }

    bool MarkdownView::TryRestoreFollowViewport(guid document, uint64_t revision, uint64_t textGeneration,
                                                uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                                double progress, double topBlend, double bottomBlend)
    {
        return ApplyFollowViewport(document, revision, textGeneration, layoutGeneration, start, end,
                                   progress, topBlend, bottomBlend, true);
    }

    bool MarkdownView::ApplyFollowViewport(guid document, uint64_t revision, uint64_t textGeneration,
                                           uint64_t layoutGeneration, uint64_t start, uint64_t end,
                                           double progress, double topBlend, double bottomBlend, bool requirePresented)
    {
        CheckUiThread();
        if (!std::isfinite(progress) || progress < 0 || progress > 1 || start > end || end > UINT32_MAX ||
            !std::isfinite(topBlend) || topBlend < 0 || topBlend > 1 ||
            !std::isfinite(bottomBlend) || bottomBlend < 0 || bottomBlend > 1)
            throw hresult_invalid_argument();
        if (!IsPreviewLoaded() || !_documentAccepted || !_previewScroll || !CaptureViewportGeometry())
            return false;
        auto snapshot = _automationSnapshot;
        auto frame = _presented.frame;
        // A failed surface can retain current navigation. Permit a new viewport
        // intent through that geometry without reopening UIA/ReadingAvailable.
        if (!snapshot || !snapshot->main || !frame || !frame->navigation || frame->main != snapshot->main ||
            snapshot->document != document || snapshot->revision != revision || snapshot->token != textGeneration ||
            snapshot->main->TextGeneration() != textGeneration || _acceptedDocument != document ||
            _acceptedRevision != revision || _displayedDocument != document || _displayedRevision != revision ||
            frame->key.layout_generation != layoutGeneration ||
            frame->navigation->Key().layout_generation != layoutGeneration ||
            !CurrentViewportFrame(*frame, _presented.geometry))
            return false;
        snapshot->main->ValidateRange(static_cast<uint32_t>(start), 0);
        snapshot->main->ValidateRange(static_cast<uint32_t>(end), 0);
        auto first = frame->navigation->ContinuousFollowY(static_cast<uint32_t>(start));
        auto last = frame->navigation->ContinuousFollowY(static_cast<uint32_t>(end));
        auto target = (1.0 - progress) * first + progress * last + _padding;
        if (topBlend > 0)
        {
            const auto origin = frame->navigation->ContinuousFollowY(0) + _padding;
            target -= std::max(0.0, origin) * topBlend;
        }
        if (!std::isfinite(target))
            throw hresult_invalid_argument();
        auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || _automationSnapshot != snapshot ||
            !CurrentViewportFrame(*frame, _presented.geometry))
            return false;
        const auto maximum = intent.scroll.ScrollableHeight();
        if (!std::isfinite(maximum) || maximum < 0)
            throw hresult_invalid_argument();
        if (requirePresented && !ScrollExtentCurrent(intent, frame->totalHeight))
            return false;
        target = std::lerp(std::clamp(target, 0.0, maximum), maximum, bottomBlend);
        int64_t targetRow{}, currentRow{};
        double targetY{}, currentY{};
        if (!::Notepads::Markdown::Native::QuantizeViewportY(target, _viewportGeometry.scale, targetRow, targetY))
            throw hresult_invalid_argument();
        auto same = ::Notepads::Markdown::Native::QuantizeViewportY(_previewScroll.VerticalOffset(),
                                                             _viewportGeometry.scale, currentRow, currentY) &&
                    currentRow == targetRow;
        // Once the identity, layout and extent guards accept a follow, it
        // replaces any deferred disclosure reveal even at the same physical row.
        _pendingReadingReveal.reset();
        if (!same)
        {
            CancelViewportRestore(target);
            _previewScroll.ChangeView(nullptr, target, nullptr, true);
        }
        else if (!requirePresented || (ReadingAvailable() && _presented.frame->viewportY == targetY))
            CancelViewportRestore();
        // ChangeView can complete later. Existing ViewChanged coalesces the real
        // offset; true acknowledges admission, never a completed publication.
        StartPreviewFrame();
        return !requirePresented || (same && ReadingAvailable() && _presented.frame->viewportY == targetY);
    }
    std::pair<uint32_t, uint32_t> MarkdownView::SelectionBounds() const noexcept
    {
        return {std::min(_selectionAnchor, _selectionActive), std::max(_selectionAnchor, _selectionActive)};
    }
    Markdown::MarkdownSelection MarkdownView::GetSelection() const
    {
        CheckUiThread();
        if (!ReadingAvailable())
            return make<implementation::MarkdownSelection>(false, guid{}, 0, 0, 0, 0);
        auto [start, end] = SelectionBounds();
        return make<implementation::MarkdownSelection>(true, _automationSnapshot->document,
                                                       _automationSnapshot->revision, _automationSnapshot->token, start,
                                                       end - start);
    }
    void MarkdownView::SelectRange(guid document, uint64_t revision, uint64_t textGeneration, uint64_t start,
                                   uint64_t length)
    {
        CheckUiThread();
        if (!ReadingAvailable() || document != _automationSnapshot->document ||
            revision != _automationSnapshot->revision || textGeneration != _automationSnapshot->token ||
            start > UINT32_MAX || length > UINT32_MAX || start + length > UINT32_MAX)
            throw hresult_invalid_argument();
        SelectReadingRange(*_automationSnapshot, static_cast<uint32_t>(start), static_cast<uint32_t>(start + length));
    }
    static void SetReadingClipboardText(std::wstring_view text)
    {
        Windows::ApplicationModel::DataTransfer::DataPackage package;
        package.SetText(hstring{text});
        Windows::ApplicationModel::DataTransfer::Clipboard::SetContent(package);
        Windows::ApplicationModel::DataTransfer::Clipboard::Flush();
    }

    void MarkdownView::RetireCodeCopyButton() noexcept
    {
        _codeCopyPresentation = 0;
        try
        {
            if (!_owningDispatcher || !_owningDispatcher.HasThreadAccess())
                return;
            auto button = std::exchange(_codeCopyButton, nullptr);
            _codeCopyTarget.reset();
            _codeCopyClick.revoke();
            _codeCopyWheel.revoke();
            _codeCopyFocusLoaded.revoke();
            if (button)
            {
                button.IsEnabled(false);
                button.Visibility(Visibility::Collapsed);
                button.ReleasePointerCaptures();
                if (_viewportOverlay)
                {
                    auto children = _viewportOverlay.Children();
                    uint32_t index{};
                    if (children.IndexOf(button, index))
                        children.RemoveAt(index);
                }
            }
        }
        catch (...)
        {
        }
    }

    void MarkdownView::HideCodeCopyButton() noexcept
    {
        _codeCopyPresentation = 0;
        _codeCopyFocusLoaded.revoke();
        try
        {
            if (auto button = _codeCopyButton; button && button.Visibility() == Visibility::Visible)
            {
                button.Visibility(Visibility::Collapsed);
                if (auto captures = button.PointerCaptures(); captures && captures.Size())
                    button.ReleasePointerCaptures();
            }
        }
        catch (...)
        {
        }
    }

    bool MarkdownView::CodeCopyViewportCurrent() const
    {
        int64_t row{};
        double paintedY{};
        return !_diagramInspection && _previewScroll && ReadingAvailable() &&
               ::Notepads::Markdown::Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _presented.geometry.scale,
                                                           row, paintedY) &&
               paintedY == _presented.frame->viewportY;
    }

    void MarkdownView::UpdateCodeCopyButton(bool preferCaret) noexcept
    {
        using ::Notepads::Markdown::Native::CodeBlockStyle;
        auto button = _codeCopyButton;
        auto presentation = _presented.serial;
        try
        {
            if (!_codeCopyTemplate || !_viewportOverlay)
                return;
            if (_dragging || !CodeCopyViewportCurrent())
            {
                HideCodeCopyButton();
                return;
            }
            auto frame = _presented.frame;
            auto geometry = _presented.geometry;

            auto point = _codeCopyPointer ? ControlToViewport(*_codeCopyPointer) : Point{-1, -1};
            bool pointerInside = point.X >= 0 && point.X < geometry.width && point.Y >= 0 && point.Y < geometry.height;
            bool buttonFocused = button && button.Visibility() == Visibility::Visible &&
                                 (button.FocusState() == FocusState::Keyboard ||
                                  (_codeCopyFocusLoaded && FocusState() == FocusState::Keyboard)) &&
                                 _codeCopyTarget && _codeCopyTarget->textGeneration == _automationSnapshot->token;
            bool caretFocused = FocusState() == FocusState::Keyboard && _selectionAnchor == _selectionActive &&
                                _focusedAction.kind == ::Notepads::Markdown::Native::ReadingActionKind::None;
            std::optional<CodeCopyTarget> target;
            double left{}, top{};
            for (auto const &visual : frame->Blocks())
            {
                auto const &block = visual.block;
                if (block.kind != 3 || block.logical_start >= block.logical_end || !CodeBlockStyle::HasCopySpace(static_cast<float>(block.width)))
                    continue;
                const auto x = block.x + _padding;
                const auto y = block.y - frame->viewportY + _padding;
                const auto right = x + block.width;
                const auto bottom = y + block.height;
                const auto buttonX = right - CodeBlockStyle::buttonInset - CodeBlockStyle::buttonSize;
                const auto verticalInset = std::clamp((block.height - CodeBlockStyle::buttonSize) * 0.5,
                                                     0.0, static_cast<double>(CodeBlockStyle::buttonInset));
                const auto buttonY = std::max(verticalInset, y + verticalInset);
                if (buttonX < 0 || buttonX + CodeBlockStyle::buttonSize > geometry.width ||
                    buttonY + CodeBlockStyle::buttonSize > std::min(bottom, geometry.height))
                    continue;

                bool hovered = pointerInside && point.X >= x && point.X < right && point.Y >= y && point.Y < bottom;
                bool focused = buttonFocused && _codeCopyTarget->node == block.node;
                bool atCaret = !buttonFocused && caretFocused && _selectionActive >= block.logical_start &&
                               _selectionActive <= block.logical_end;
                const bool preferred = preferCaret ? atCaret : hovered;
                if (buttonFocused ? !focused : !preferred && (target || (!atCaret && !hovered)))
                    continue;
                target = CodeCopyTarget{_automationSnapshot->token, block.node, static_cast<uint32_t>(block.logical_start),
                                        static_cast<uint32_t>(block.logical_end)};
                left = geometry.overlayX + buttonX;
                top = geometry.overlayY + buttonY;
                if (focused || preferred)
                    break;
            }
            if (!target)
            {
                HideCodeCopyButton();
                return;
            }
            if (button && _codeCopyTarget == target && _codeCopyPresentation == presentation)
                return;
            if (button && _codeCopyTarget != target)
            {
                auto captures = button.PointerCaptures();
                if (button.IsPressed() || (captures && captures.Size()))
                {
                    HideCodeCopyButton();
                    return;
                }
                // The old XAML peer and any pending clicks keep their old button identity.
                RetireCodeCopyButton();
                if (_presented.serial != presentation || _codeCopyButton || !CodeCopyViewportCurrent())
                    return;
                button = nullptr;
            }
            if (!button)
            {
                auto overlay = _viewportOverlay;
                button = _codeCopyTemplate.LoadContent().try_as<Button>();
                if (!button || overlay != _viewportOverlay || _presented.serial != presentation ||
                    _codeCopyButton || !CodeCopyViewportCurrent())
                    return;
                button.Visibility(Visibility::Collapsed);
                button.Width(CodeBlockStyle::buttonSize);
                button.Height(CodeBlockStyle::buttonSize);
                _codeCopyButton = button;
                _codeCopyTarget = target;
                _codeCopyClick = button.Click(auto_revoke, [weak = get_weak()](auto const &sender, auto const &args)
                {
                    if (auto owner = weak.get())
                        owner->OnCodeCopyClick(sender, args);
                });
                AttachWheelScrollEvents();
                overlay.Children().Append(button);
                if (_codeCopyButton != button || _codeCopyTarget != target || _presented.serial != presentation)
                    return;
            }
            _codeCopyPresentation = presentation;
            auto current = [&]
            {
                return _codeCopyButton == button && _presented.serial == presentation && _codeCopyTarget == target &&
                       CodeCopyViewportCurrent();
            };
            Canvas::SetLeft(button, left);
            if (!current())
                return;
            Canvas::SetTop(button, top);
            if (!current())
                return;
            if (current())
                button.Visibility(Visibility::Visible);
        }
        catch (...)
        {
            if (_codeCopyButton == button && _presented.serial == presentation)
                RetireCodeCopyButton();
        }
    }

    void MarkdownView::OnCodeCopyClick(IInspectable const &sender, RoutedEventArgs const &)
    {
        CopyCodeBlock(sender, SetReadingClipboardText);
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    void MarkdownView::CopyCodeBlockForDiagnostics(bool clipboardSucceeds)
    {
        CheckUiThread();
        CopyCodeBlock(_codeCopyButton, clipboardSucceeds ? +[](std::wstring_view) {}
                                                      : +[](std::wstring_view) { throw hresult_error(E_FAIL); });
    }
#endif

    void MarkdownView::CopyCodeBlock(IInspectable const &sender, void (*writeClipboard)(std::wstring_view))
    {
        auto lifetime = get_strong();
        auto target = _codeCopyTarget;
        auto button = _codeCopyButton;
        if (!button || sender != button || !target || _codeCopyPresentation != _presented.serial ||
            !CodeCopyViewportCurrent() || target->textGeneration != _automationSnapshot->token)
            return;
        try
        {
            auto snapshot = _automationSnapshot;
            auto text = snapshot->main->Text();
            writeClipboard(text.substr(target->start, target->end - target->start));
        }
        catch (...)
        {
            return;
        }
        try
        {
            if (button == _codeCopyButton && _codeCopyTarget == target)
            {
                if (auto icon = button.Content().try_as<SymbolIcon>())
                    icon.Symbol(Symbol::Accept);
            }
        }
        catch (...) {}
        try { _codeCopied(*this, nullptr); }
        catch (...) { /* Clipboard success is independent of host notification. */ }
    }

    event_token MarkdownView::CodeCopied(Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, IInspectable> const &handler)
    {
        return _codeCopied.add(handler);
    }

    void MarkdownView::CodeCopied(event_token const &token) noexcept
    {
        _codeCopied.remove(token);
    }

    bool MarkdownView::CopySelection()
    {
        CheckUiThread();
        if (!ReadingAvailable())
            return false;
        auto [start, end] = SelectionBounds();
        if (start == end)
            return false;
        try
        {
            auto content = _automationSnapshot->descriptors->Copy(*_automationSnapshot->main, start, end);
            SetReadingClipboardText({content.data(), content.size()});
            return true;
        }
        catch (...)
        {
            return false;
        }
    }
    event_token MarkdownView::SelectionChanged(
        Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownSelection> const &handler)
    {
        return _selectionChanged.add(handler);
    }
    void MarkdownView::SelectionChanged(event_token const &token) noexcept
    {
        _selectionChanged.remove(token);
    }
    event_token MarkdownView::LinkRequested(
        Windows::Foundation::TypedEventHandler<Markdown::MarkdownView, Markdown::MarkdownLinkRequestedEventArgs> const &handler)
    {
        return _linkRequested.add(handler);
    }
    void MarkdownView::LinkRequested(event_token const &token) noexcept
    {
        _linkRequested.remove(token);
    }
    bool MarkdownView::CurrentPreviewGeometry(::Notepads::Markdown::Native::PreviewFrame const &frame) const
    {
        return _presented.frame.get() == &frame && CurrentViewportFrame(frame, _presented.geometry);
    }
    bool MarkdownView::CurrentViewportFrame(::Notepads::Markdown::Native::PreviewFrame const &frame,
                                            ::Notepads::Markdown::Native::ViewportGeometry const &geometry) const
    {
        // Candidate Frames are drawn before the first automation snapshot is
        // installed. Validate the source owner, never a previous UIA snapshot.
        if (!_documentAccepted || !_loaded || !_readingSource || !frame.main || frame.main != _readingSource->Main() ||
            frame.main->SourceGeneration() != _previewSourceGeneration ||
            frame.main->TextGeneration() != _readingSource->textGeneration ||
            frame.key.layout_generation != _previewLayoutGeneration ||
            frame.key.theme_generation != _effectiveThemeGeneration ||
            frame.token != _previewToken->load(std::memory_order_acquire) ||
            _viewportTemplateState != ViewportTemplateState::Ready || geometry.epoch != _viewportGeometry.epoch ||
            !CurrentImageFrame(frame) || !CurrentMathFrame(frame) || !CurrentDiagramFrame(frame))
            return false;
        return geometry.width > 2 * _padding && geometry.height > 0 &&
               std::abs(frame.viewportHeight - geometry.height) <= 0.25 &&
               std::abs(frame.key.width - (geometry.width - 2 * _padding)) <= 0.25;
    }
    ::Notepads::Markdown::Native::ReadingNavigationIndex const &MarkdownView::ReadingNavigation(
        TextSnapshot const &snapshot) const
    {
        CheckLiveRange(snapshot);
        if (!_presented.frame || !_presented.frame->navigation || _presented.frame->main != snapshot.main ||
            !CurrentPreviewGeometry(*_presented.frame))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return *_presented.frame->navigation;
    }
    void MarkdownView::SelectReadingRange(TextSnapshot const &snapshot, uint32_t start, uint32_t end, bool notify)
    {
        CheckLiveRange(snapshot);
        if (!snapshot.main || !snapshot.Bounds().Contains(start, end))
            throw hresult_invalid_argument();
        snapshot.main->ValidateRange(start, end - start);
        auto const &navigation = ReadingNavigation(snapshot);
        auto action = CaptureReadingAction();
        auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || !CurrentReadingAction(action))
            return;
        // Empty source matches remain empty even inside a shaped cluster.
        if (ReadingRangeHidden(snapshot, {start, end}) == false)
            navigation.SnapSelection(start, end);
        UpdateSelection(start, end, notify);
    }
    void MarkdownView::RemoveReadingSelection(TextSnapshot const &snapshot, uint32_t start, uint32_t end)
    {
        CheckLiveRange(snapshot);
        auto [selectedStart, selectedEnd] = SelectionBounds();
        if (start == selectedStart && end == selectedEnd)
        {
            // These endpoints already belong to the current snapped selection.
            // Clearing it does not require new font or viewport geometry.
            UpdateSelection(selectedStart, selectedStart);
        }
    }
    MarkdownView::ReadingAction MarkdownView::CaptureReadingAction() const
    {
        ReadingAction action{_presented.frame, _automationSnapshot, _presented.serial};
        if (!CurrentReadingAction(action))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return action;
    }
    bool MarkdownView::CurrentReadingAction(ReadingAction const &action) const
    {
        return action.frame && action.snapshot && ReadingAvailable() && action.frame == _presented.frame &&
               action.snapshot == _automationSnapshot && action.frame->main == action.snapshot->main &&
               action.presentation == _presented.serial;
    }
    void MarkdownView::InvalidateVerticalNavigation() noexcept
    {
        _hasPreferredCaretX = false;
        _pendingCaretLine.reset();
        _pendingCaretFrameRequested = false;
    }
    bool MarkdownView::UpdateSelection(uint32_t anchor, uint32_t active, bool notify, SelectionMotion motion,
                                       bool trailingCaret)
    {
        auto lifetime = get_strong();
        if (!ReadingAvailable())
            return false;
        // Logical selection removal is allowed while geometry is pending.
        // Keep its owners alive without claiming that Frame is actionable.
        ReadingAction action{_presented.frame, _automationSnapshot, _presented.serial};
        if (anchor == active && action.frame && action.frame->main == action.snapshot->main && action.frame->navigation)
            anchor = active = action.frame->navigation->CanonicalCaret(active);
        _automationSnapshot->main->ValidateRange(anchor, 0);
        _automationSnapshot->main->ValidateRange(active, 0);
        if (motion != SelectionMotion::Vertical)
            InvalidateVerticalNavigation();
        const bool logicalChange = _selectionAnchor != anchor || _selectionActive != active;
        if (!logicalChange && _readingCaretTrailing == trailingCaret)
            return CurrentReadingAction(action);
        _selectionAnchor = anchor;
        _selectionActive = active;
        _readingCaretTrailing = trailingCaret;
        DrawPreview();
        if (notify && logicalChange)
        {
            try
            {
                _selectionChanged(*this, GetSelection());
            }
            catch (...)
            {
            }
            // UIA selection notification is logical: it is still required when
            // RemoveFromSelection runs during pending geometry. Only the same
            // current text and the selection just published may be notified.
            if (!ReadingAvailable() || action.snapshot != _automationSnapshot ||
                _selectionAnchor != anchor || _selectionActive != active)
                return false;
            try
            {
                auto peer = Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer::FromElement(*this);
                if (peer)
                    peer.RaiseAutomationEvent(
                        Windows::UI::Xaml::Automation::Peers::AutomationEvents::TextPatternOnTextSelectionChanged);
            }
            catch (...)
            {
            }
        }
        return CurrentReadingAction(action) && _selectionAnchor == anchor && _selectionActive == active &&
               _readingCaretTrailing == trailingCaret;
    }
    void MarkdownView::RaiseReadingTextChanged()
    {
        auto lifetime = get_strong();
        auto snapshot = _automationSnapshot;
        try
        {
            auto peer = Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer::FromElement(*this);
            if (peer)
            {
                peer.InvalidatePeer();
                if (!ReadingAvailable() || snapshot != _automationSnapshot)
                    return;
                peer.RaiseAutomationEvent(
                    Windows::UI::Xaml::Automation::Peers::AutomationEvents::TextPatternOnTextChanged);
                if (!ReadingAvailable() || snapshot != _automationSnapshot)
                    return;
                peer.RaiseAutomationEvent(Windows::UI::Xaml::Automation::Peers::AutomationEvents::StructureChanged);
            }
        }
        catch (...)
        {
        }
    }
    void MarkdownView::NotifyDisclosureChanges(ReadingAction const &action,
        ::Notepads::Markdown::Native::ReadingVisibility const &previous)
    {
        auto const &current = action.frame->visibility;
        if (!current || current->disclosures.size() != previous.disclosures.size())
            return;
        bool changed{};
        auto peers = _readingPeers;
        auto const &desc = *action.snapshot->descriptors;
        using namespace Windows::UI::Xaml::Automation;
        for (size_t i = 0; i < current->disclosures.size(); ++i)
        {
            auto const &before = previous.disclosures[i];
            auto const &after = current->disclosures[i];
            if (before.expanded == after.expanded || before.node != after.node)
                continue;
            if (!CurrentReadingAction(action))
                return;
            changed = true;
            if (!peers || peers->snapshot != action.snapshot ||
                !ReadingChildVisible(*action.snapshot, desc.disclosures[i].text))
                continue;
            auto peer = peers->peers[desc.objects.size() + desc.links.size() + i];
            if (!peer)
                continue;
            try
            {
                auto state = [](uint32_t expanded) { return expanded ? ExpandCollapseState::Expanded : ExpandCollapseState::Collapsed; };
                peer->RaisePropertyChangedEvent(ExpandCollapsePatternIdentifiers::ExpandCollapseStateProperty(),
                                                box_value(state(before.expanded)), box_value(state(after.expanded)));
            }
            catch (...)
            {
            }
        }
        if (changed && CurrentReadingAction(action))
        {
            try
            {
                auto peer = Peers::FrameworkElementAutomationPeer::FromElement(*this);
                if (peer)
                {
                    peer.InvalidatePeer();
                    if (CurrentReadingAction(action))
                        peer.RaiseAutomationEvent(Peers::AutomationEvents::StructureChanged);
                }
            }
            catch (...)
            {
            }
        }
    }
    ::Notepads::Markdown::Native::ReadingVector<::Notepads::Markdown::Native::ReadingRect> MarkdownView::ReadingRectangles(
        TextSnapshot const &snapshot, uint32_t start, uint32_t end) const
    {
        ReadingNavigation(snapshot);
        if (!snapshot.Bounds().Contains(start, end))
            throw hresult_invalid_argument();
        if (auto inspect = DiagramInspectionReadingRect(snapshot))
        {
            using ::Notepads::Markdown::Native::ReadingAllocator;
            using ::Notepads::Markdown::Native::ReadingRect;
            using ::Notepads::Markdown::Native::ReadingVector;
            ReadingVector<ReadingRect> visible{ReadingAllocator<ReadingRect>{snapshot.main->Memory()}};
            if (start < inspect->end && end > inspect->start)
                visible.push_back(*inspect);
            return visible;
        }
        auto rectangles = ::Notepads::Markdown::Native::NativeDocumentRenderer::Rectangles(*_presented.frame, start, end);
        for (auto &rect : rectangles)
            rect = ClipReadingRectangle(rect);
        rectangles.erase(
            std::remove_if(rectangles.begin(), rectangles.end(), [](auto r) { return r.width <= 0 || r.height <= 0; }),
            rectangles.end());
        return rectangles;
    }
    ::Notepads::Markdown::Native::ReadingRect MarkdownView::ClipReadingRectangle(
        ::Notepads::Markdown::Native::ReadingRect rect) const noexcept
    {
        auto const &geometry = _presented.geometry;
        auto left = std::max(0.0f, rect.x + _padding);
        auto top = std::max(0.0f, rect.y + _padding);
        auto right = std::min(static_cast<float>(geometry.width), rect.x + _padding + rect.width);
        auto bottom = std::min(static_cast<float>(geometry.height), rect.y + _padding + rect.height);
        rect.x = left + static_cast<float>(geometry.x);
        rect.y = top + static_cast<float>(geometry.y);
        rect.width = std::max(0.0f, right - left);
        rect.height = std::max(0.0f, bottom - top);
        return rect;
    }

    Windows::Foundation::Rect MarkdownView::ReadingTableRectangle(TextSnapshot const &snapshot, uint32_t node) const
    {
        ReadingNavigation(snapshot);
        if (DiagramInspectionReadingRect(snapshot))
            return {};
        auto const &frame = *_presented.frame;
        auto const &blocks = frame.Blocks();
        auto at = std::find_if(blocks.begin(), blocks.end(), [node](auto const &visual)
        {
            return visual.block.node == node && visual.htmlTable;
        });
        if (at == blocks.end())
            return {};
        auto rect = ClipReadingRectangle({static_cast<float>(at->block.x),
            static_cast<float>(at->block.y - frame.viewportY),
            at->tableWidth, static_cast<float>(at->measuredHeight), 0, 0});
        return rect.width > 0 && rect.height > 0 ? ReadingScreenRectangle(rect) : Windows::Foundation::Rect{};
    }

    Windows::Foundation::Rect MarkdownView::ReadingScreenRectangle(
        ::Notepads::Markdown::Native::ReadingRect const &rectangle) const
    {
        auto peer = Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer::FromElement(*this);
        if (!peer)
            peer = Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer::CreatePeerForElement(*this);
        auto bounds = peer.GetBoundingRectangle();
        auto scaleX = ActualWidth() > 0 ? bounds.Width / ActualWidth() : 1.0;
        auto scaleY = ActualHeight() > 0 ? bounds.Height / ActualHeight() : scaleX;
        return {static_cast<float>(bounds.X + rectangle.x * scaleX),
                static_cast<float>(bounds.Y + rectangle.y * scaleY), static_cast<float>(rectangle.width * scaleX),
                static_cast<float>(rectangle.height * scaleY)};
    }
    uint32_t MarkdownView::ReadingPositionFromScreen(TextSnapshot const &snapshot, Point point) const
    {
        ReadingNavigation(snapshot);
        if (!std::isfinite(point.X) || !std::isfinite(point.Y))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        if (auto inspect = DiagramInspectionReadingRect(snapshot))
        {
            auto bounds = ReadingScreenRectangle(*inspect);
            if (point.X < bounds.X || point.Y < bounds.Y ||
                point.X >= bounds.X + bounds.Width || point.Y >= bounds.Y + bounds.Height)
                throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
            return point.X < bounds.X + bounds.Width / 2 ? inspect->start : inspect->end;
        }
        auto bounds =
            ReadingScreenRectangle({0, 0, static_cast<float>(ActualWidth()), static_cast<float>(ActualHeight()), 0, 0});
        if (!(bounds.Width > 0) || !(bounds.Height > 0))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto scaleX = bounds.Width / std::max(1.0, ActualWidth());
        auto scaleY = bounds.Height / std::max(1.0, ActualHeight());
        auto hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::HitTest(
            *_presented.frame, static_cast<float>((point.X - bounds.X) / scaleX - _presented.geometry.x - _padding),
            static_cast<float>((point.Y - bounds.Y) / scaleY - _presented.geometry.y - _padding));
        if (!hit)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return hit->position;
    }
    ::Notepads::Markdown::Native::ReadingVector<::Notepads::Markdown::Native::ReadingSpan> MarkdownView::VisibleReadingRanges(
        TextSnapshot const &snapshot) const
    {
        ReadingNavigation(snapshot);
        auto bounds = snapshot.Bounds();
        auto rectangles = ReadingRectangles(snapshot, bounds.start, bounds.end);
        ::Notepads::Markdown::Native::ReadingVector<::Notepads::Markdown::Native::ReadingSpan> result{
            ::Notepads::Markdown::Native::ReadingAllocator<::Notepads::Markdown::Native::ReadingSpan>{snapshot.main->Memory()}};
        for (auto const &rect : rectangles)
            result.push_back({rect.start, rect.end});
        std::sort(result.begin(), result.end(), [](auto a, auto b) { return a.start < b.start; });
        size_t output{};
        for (auto range : result)
        {
            if (output && range.start <= result[output - 1].end)
                result[output - 1].end = std::max(result[output - 1].end, range.end);
            else
                result[output++] = range;
        }
        result.resize(output);
        return result;
    }
    void MarkdownView::RevealReadingRange(TextSnapshot const &snapshot, uint32_t start, uint32_t end, bool alignTop,
                                          bool externalScroll, bool expandDisclosures)
    {
        ReadingNavigation(snapshot);
        if (!snapshot.Bounds().Contains(start, end))
            throw hresult_invalid_argument();
        snapshot.main->ValidateRange(start, end - start);
        if (_previewScroll)
            RequestReadingReveal(snapshot.token, {start, end}, 0, alignTop, externalScroll, expandDisclosures);
    }
    void MarkdownView::InvokeReadingLink(TextSnapshot const &snapshot, size_t index)
    {
        auto lifetime = get_strong();
        CheckLiveRange(snapshot);
        if (!snapshot.descriptors || index >= snapshot.descriptors->links.size())
            throw hresult_invalid_argument();
        auto current = _automationSnapshot;
        auto const &link = current->descriptors->links[index];
        if (!ReadingChildVisible(snapshot, link.text))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto destination = hstring{std::wstring_view(link.destination.data(), link.destination.size())};
        auto title = hstring{std::wstring_view(link.title.data(), link.title.size())};
        auto args = make<implementation::MarkdownLinkRequestedEventArgs>(
            current->document, current->revision, current->token, link.text.start, link.text.end - link.text.start,
            std::move(destination), std::move(title), link.localPosition, link.localHeadingNode, link.wiki);
        _linkRequested(*this, args);
    }

    ScrollViewer MarkdownView::ReadingScrollViewer() const
    {
        CheckUiThread();
        if (_loaded && !_documentAccepted)
            return nullptr;
        if (!_loaded || !_previewScroll || !_previewScroll.IsLoaded() ||
            GetTemplateChild(L"PreviewScrollViewer") != _previewScroll)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return _previewScroll;
    }
    void MarkdownView::DetachReadingScrollEvents() noexcept
    {
        try
        {
            if (!_owningDispatcher || !_owningDispatcher.HasThreadAccess() || !_previewScroll)
                return;
            using Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer;
            auto target = FrameworkElementAutomationPeer::FromElement(_previewScroll);
            auto owner = FrameworkElementAutomationPeer::FromElement(*this);
            if (target && owner && target.EventsSource() == owner)
                target.EventsSource(nullptr);
        }
        catch (...)
        {
        }
    }
    com_ptr<implementation::ReadingObjectProvider> MarkdownView::ReadingPeer(
        std::shared_ptr<TextSnapshot const> const &snapshot, ReadingChildKind kind, size_t index)
    {
        CheckLiveRange(*snapshot);
        auto const &desc = *snapshot->descriptors;
        if (!ReadingChildVisible(*snapshot, desc.Child(kind, index)))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        if (!_readingPeers || _readingPeers->snapshot->token != snapshot->token)
            _readingPeers = std::allocate_shared<ReadingPeerCache>(
                ::Notepads::Markdown::Native::ReadingAllocator<ReadingPeerCache>{snapshot->main->Memory()}, snapshot);
        auto slot = index;
        switch (kind)
        {
        case ReadingChildKind::Object:
            break;
        case ReadingChildKind::Link:
            slot += desc.objects.size();
            break;
        case ReadingChildKind::Disclosure:
            slot += desc.objects.size() + desc.links.size();
            break;
        case ReadingChildKind::Table:
        case ReadingChildKind::Caption:
            slot += desc.objects.size() + desc.links.size() + desc.disclosures.size();
            if (kind == ReadingChildKind::Caption)
                slot += desc.tables.size();
            break;
        }
        auto &peer = _readingPeers->peers[slot];
        if (!peer)
            peer = make_self<implementation::ReadingObjectProvider>(get_weak(), snapshot, kind, index);
        return peer;
    }
    Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple MarkdownView::ReadingObjectProvider(
        std::shared_ptr<TextSnapshot const> const &snapshot, ReadingChildKind kind, size_t index)
    {
        if (auto peer = ReadingNotePeer(*snapshot, kind, index).try_as<Markdown::MarkdownViewAutomationPeer>())
            return winrt::get_self<implementation::MarkdownViewAutomationPeer>(peer)->Raw();
        return ReadingPeer(snapshot, kind, index)->Raw();
    }

    std::optional<bool> MarkdownView::ReadingRangeHidden(TextSnapshot const &snapshot, ReadingSpan span) const
    {
        ReadingNavigation(snapshot);
        auto const &visibility = _presented.frame->visibility;
        if (!visibility)
            return false;
        if (span.start == span.end)
            return visibility->HiddenAt(span.start) != nullptr;
        if (auto hidden = visibility->FirstHidden(span))
        {
            if (hidden->start <= span.start && span.end <= hidden->end)
                return true;
            return {};
        }
        return false;
    }

    bool MarkdownView::ReadingChildVisible(TextSnapshot const &snapshot, ReadingSpan span) const
    {
        return !ReadingRangeHidden(snapshot, span).value_or(true);
    }

    bool MarkdownView::ReadingDisclosureFocused(TextSnapshot const &snapshot, size_t index) const
    {
        return ReadingChildVisible(snapshot, snapshot.descriptors->Child(ReadingChildKind::Disclosure, index)) &&
               _readingFocused && _focusedAction.kind == ::Notepads::Markdown::Native::ReadingActionKind::Disclosure &&
               _focusedAction.index == index;
    }

    bool MarkdownView::ReadingDisclosureExpanded(TextSnapshot const &snapshot, size_t index) const
    {
        if (!ReadingChildVisible(snapshot, snapshot.descriptors->Child(ReadingChildKind::Disclosure, index)))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto const &visibility = _presented.frame->visibility;
        auto row = visibility ? visibility->Disclosure(snapshot.descriptors->disclosures[index].node) : nullptr;
        if (!row)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return row->expanded != 0;
    }

    void MarkdownView::ApplyDisclosureOverride(DisclosureOverrides &overrides, uint32_t node, bool expanded)
    {
        auto entry = std::lower_bound(overrides.begin(), overrides.end(), node,
                                     [](auto const &row, uint32_t id) { return row.node < id; });
        auto state = static_cast<uint32_t>(expanded);
        if (entry != overrides.end() && entry->node == node)
            entry->expanded = state;
        else
            overrides.insert(entry, {node, state});
    }

    void MarkdownView::InstallDisclosureOverrides(std::shared_ptr<DisclosureOverrides const> overrides)
    {
        _diagramInspection.reset();
        _pendingDiagramInspection.reset();
        _disclosureOverrides = std::move(overrides);
        StopReadingDrag();
        ClearReadingHover();
        StartPreviewLayout();
    }

    void MarkdownView::SetReadingDisclosure(TextSnapshot const &snapshot, size_t index, std::optional<bool> expanded)
    {
        auto current = ReadingDisclosureExpanded(snapshot, index);
        auto next = expanded.value_or(!current);
        if (current == next)
            return;
        _pendingReadingReveal.reset();
        auto overrides = std::allocate_shared<DisclosureOverrides>(
            ::Notepads::Markdown::Native::ReadingAllocator<DisclosureOverrides>{snapshot.main->Memory()},
            ::Notepads::Markdown::Native::ReadingAllocator<notepads_markdown_preview_disclosure_override>{snapshot.main->Memory()});
        if (_disclosureOverrides)
            *overrides = *_disclosureOverrides;
        ApplyDisclosureOverride(*overrides, snapshot.descriptors->disclosures[index].node, next);
        InstallDisclosureOverrides(std::move(overrides));
    }

    bool MarkdownView::ExpandReadingDisclosures(ReadingSpan span, uint32_t node)
    {
        auto action = CaptureReadingAction();
        auto const &visibility = action.frame->visibility;
        if (!visibility || (!node && !(span.start == span.end ? visibility->HiddenAt(span.start) : visibility->FirstHidden(span))))
            return false;
        auto memory = action.snapshot->main->Memory();
        auto overrides = std::allocate_shared<DisclosureOverrides>(
            ::Notepads::Markdown::Native::ReadingAllocator<DisclosureOverrides>{memory},
            ::Notepads::Markdown::Native::ReadingAllocator<notepads_markdown_preview_disclosure_override>{memory});
        overrides->reserve(visibility->disclosures.size());
        bool changed{};
        for (auto const &row : visibility->disclosures)
        {
            auto contains = node ? row.first_body_node <= node && node < row.next_node :
                (row.body_start < row.body_end && (span.start == span.end ? row.body_start <= span.start && span.start < row.body_end
                                                                         : row.body_start < span.end && span.start < row.body_end));
            changed |= contains && !row.expanded;
            overrides->push_back({row.node, row.expanded || contains ? 1u : 0u});
        }
        if (changed)
            InstallDisclosureOverrides(std::move(overrides));
        return changed;
    }
    bool MarkdownView::ReadingLinkFocused(TextSnapshot const &snapshot, size_t index) const
    {
        CheckLiveRange(snapshot);
        return _readingFocused && _focusedAction.kind == ::Notepads::Markdown::Native::ReadingActionKind::Link &&
               _focusedAction.index == index;
    }
    bool MarkdownView::ReadingTaskFocused(TextSnapshot const &snapshot, size_t index) const
    {
        return ReadingTaskEnabled(snapshot, index) && _readingFocused &&
               _focusedAction.kind == ::Notepads::Markdown::Native::ReadingActionKind::Task && _focusedAction.index == index;
    }
    void MarkdownView::FocusReadingLink(TextSnapshot const &snapshot, size_t index)
    {
        if (index > UINT32_MAX)
            throw hresult_invalid_argument();
        FocusReadingAction(snapshot, {::Notepads::Markdown::Native::ReadingActionKind::Link, static_cast<uint32_t>(index)});
    }
    void MarkdownView::FocusReadingTask(TextSnapshot const &snapshot, size_t index)
    {
        if (index > UINT32_MAX)
            throw hresult_invalid_argument();
        FocusReadingAction(snapshot, {::Notepads::Markdown::Native::ReadingActionKind::Task, static_cast<uint32_t>(index)});
    }
    void MarkdownView::FocusReadingDisclosure(TextSnapshot const &snapshot, size_t index)
    {
        if (index > UINT32_MAX)
            throw hresult_invalid_argument();
        FocusReadingAction(snapshot, {::Notepads::Markdown::Native::ReadingActionKind::Disclosure, static_cast<uint32_t>(index)});
    }
    void MarkdownView::FocusReadingAction(TextSnapshot const &snapshot,
                                          ::Notepads::Markdown::Native::ReadingActionTarget target)
    {
        auto lifetime = get_strong();
        CheckLiveRange(snapshot);
        using Kind = ::Notepads::Markdown::Native::ReadingActionKind;
        if (target.kind == Kind::None || !snapshot.descriptors)
            throw hresult_invalid_argument();
        auto kind = target.kind == Kind::Link ? ReadingChildKind::Link :
                    target.kind == Kind::Disclosure ? ReadingChildKind::Disclosure : ReadingChildKind::Object;
        auto enabled = [&]
        {
            if (target.kind == Kind::Task)
                return ReadingTaskEnabled(snapshot, target.index);
            if (target.kind == Kind::Diagram)
                return ReadingDiagramEnabled(snapshot, target.index);
            return true;
        };
        if (!enabled())
            throw hresult_error(UIA_E_ELEMENTNOTENABLED);
        auto action = CaptureReadingAction();
        auto range = action.snapshot->descriptors->Child(kind, target.index);
        auto peer = ReadingPeer(action.snapshot, kind, target.index);
        InvalidateVerticalNavigation();
        if (_focusedAction != target)
        {
            _diagramInspection.reset();
            _pendingDiagramInspection.reset();
        }
        _focusedAction = target;
        Focus(Windows::UI::Xaml::FocusState::Programmatic);
        if (target.kind == Kind::Diagram && _focusedAction == target && _automationSnapshot == action.snapshot)
            _readingFocused = FocusState() != Windows::UI::Xaml::FocusState::Unfocused;
        if (!CurrentReadingAction(action) || _focusedAction != target || !enabled())
            return;
        if (target.kind != Kind::Diagram || ReadingRectangles(*action.snapshot, range.start, range.end).empty())
            RevealReadingRange(*action.snapshot, range.start, range.end, true);
        // Reveal can replace geometry while the same immutable text remains live.
        if (!ReadingAvailable() || action.snapshot != _automationSnapshot || _focusedAction != target || !enabled())
            return;
        DrawPreview();
        if (!ReadingAvailable() || action.snapshot != _automationSnapshot || _focusedAction != target || !enabled())
            return;
        try
        {
            peer->RaiseAutomationEvent(Windows::UI::Xaml::Automation::Peers::AutomationEvents::AutomationFocusChanged);
        }
        catch (...)
        {
        }
    }
    void MarkdownView::OnTaskInteractionChanged()
    {
        auto lifetime = get_strong();
        using Kind = ::Notepads::Markdown::Native::ReadingActionKind;
        if (_focusedAction.kind == Kind::Task)
            _focusedAction = {};
        if (_pressedAction.kind == Kind::Task)
            StopReadingDrag();
        // Cached adapters must observe capability changes as well as visual changes.
        auto snapshot = _automationSnapshot;
        auto peers = _readingPeers;
        if (snapshot && peers && peers->snapshot == snapshot)
        {
            auto const &objects = snapshot->descriptors->objects;
            for (size_t index = 0; index < objects.size(); ++index)
            {
                if (!ReadingAvailable() || snapshot != _automationSnapshot || peers != _readingPeers)
                    break;
                auto peer = peers->peers[index];
                if (objects[index].kind == ::Notepads::Markdown::Native::ReadingObjectKind::Task && peer)
                {
                    try
                    {
                        peer->InvalidatePeer();
                    }
                    catch (...)
                    {
                    }
                }
            }
        }
        try
        {
            auto peer = Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer::FromElement(*this);
            if (peer)
            {
                peer.InvalidatePeer();
                peer.RaiseAutomationEvent(Windows::UI::Xaml::Automation::Peers::AutomationEvents::StructureChanged);
            }
        }
        catch (...)
        {
        }
        if (ReadingAvailable())
            DrawPreview();
    }
    Windows::UI::Xaml::Automation::Peers::AutomationPeer MarkdownView::ReadingCaptionPeer(
        std::shared_ptr<TextSnapshot const> const &snapshot, size_t index)
    {
        return ReadingPeer(snapshot, ReadingChildKind::Caption, index).as<Windows::UI::Xaml::Automation::Peers::AutomationPeer>();
    }

    Windows::Foundation::Collections::IVector<Windows::UI::Xaml::Automation::Peers::AutomationPeer> MarkdownView::
        ReadingTableChildren(std::shared_ptr<TextSnapshot const> const &snapshot, size_t index, bool captionOnly)
    {
        CheckLiveRange(*snapshot);
        auto const &desc = *snapshot->descriptors;
        auto children = single_threaded_vector<Windows::UI::Xaml::Automation::Peers::AutomationPeer>();
        desc.VisitTableChildren(index, captionOnly, [&](ReadingChildKind kind, size_t child)
        {
            if (ReadingChildVisible(*snapshot, desc.Child(kind, child)))
                children.Append(ReadingPeer(snapshot, kind, child).as<Windows::UI::Xaml::Automation::Peers::AutomationPeer>());
            return true;
        });
        return children;
    }

    Windows::Foundation::Collections::IVector<Windows::UI::Xaml::Automation::Peers::AutomationPeer> MarkdownView::
        ReadingChildren()
    {
        CheckUiThread();
        if (!ReadingAvailable())
            return nullptr;
        auto snapshot = AutomationSnapshot();
        auto result = single_threaded_vector<Windows::UI::Xaml::Automation::Peers::AutomationPeer>();
        auto const &desc = *snapshot->descriptors;
        desc.VisitRootChildren([&](ReadingChildKind kind, size_t index)
        {
            if (auto peer = ReadingNotePeer(*snapshot, kind, index))
                result.Append(peer);
            else if (ReadingChildVisible(*snapshot, desc.Child(kind, index)))
                result.Append(ReadingPeer(snapshot, kind, index).as<Windows::UI::Xaml::Automation::Peers::AutomationPeer>());
            return true;
        });
        if (_codeCopyButton && _codeCopyButton.Visibility() == Visibility::Visible && CodeCopyViewportCurrent())
        {
            using Windows::UI::Xaml::Automation::Peers::FrameworkElementAutomationPeer;
            auto button = FrameworkElementAutomationPeer::FromElement(_codeCopyButton);
            if (!button)
                button = FrameworkElementAutomationPeer::CreatePeerForElement(_codeCopyButton);
            if (button)
                result.Append(button);
        }
        return result;
    }

}
