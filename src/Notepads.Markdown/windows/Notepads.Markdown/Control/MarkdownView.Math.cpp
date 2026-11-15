// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "notepads/markdown/math.h"

namespace winrt::Notepads::Markdown::implementation
{
    namespace Native = ::Notepads::Markdown::Native;

    void MarkdownView::EnsureGraphicsFactory()
    {
        CheckUiThread();
        if (!_d2dFactory)
        {
            D2D1_FACTORY_OPTIONS options{};
            check_hresult(::D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, options, _d2dFactory.put()));
        }
    }

    Native::NativeMathKey MarkdownView::CaptureMathKey() const
    {
        Native::NativeMathKey key;
        if ((static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH) == 0 || !_readingSource)
            return key;
        key.identity = _readingSource->memory->Identity();
        key.sourceGeneration = _previewSourceGeneration;
        key.textGeneration = _readingSource->textGeneration;
        key.attempt = _mathAttempt;
        key.factoryEpoch = _mathFactoryEpoch;
        // Private engine/font/monochrome-style revisions. Width, DPI, foreground
        // and theme size affect placement only, never the immutable em artifact.
        key.engine = key.fonts = key.style = 1;
        return key;
    }

    Native::PreviewMathSnapshot MarkdownView::CaptureMath() const
    {
        Native::PreviewMathSnapshot snapshot;
        snapshot.enabled = (static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH) != 0;
        snapshot.presentations = _mathPresentations;
        snapshot.generation = _mathGeneration;
        if (!snapshot.enabled || !_readingSource)
            return snapshot;
        snapshot.key = CaptureMathKey();
        if (_mathBound && _mathCoordinator)
            snapshot.attempts = _mathCoordinator->AttemptStates(_mathAttempt);
        else if (_mathBound)
            snapshot.attempts.fill(Native::NativeMathState::Unavailable);
        return snapshot;
    }

    bool MarkdownView::CurrentMathFrame(Native::PreviewFrame const &frame) const noexcept
    {
        auto enabled = (static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH) != 0;
        if (frame.math.enabled != enabled)
            return false;
        if (!enabled)
            return true;
        // Deliveries only append within this attempt. Keep the layout and
        // its frozen formula snapshot usable until their replacement publishes.
        if (frame.math.generation > _mathGeneration)
            return false;
        return frame.math.key.attempt == _mathAttempt && frame.math.key.factoryEpoch == _mathFactoryEpoch &&
               frame.math.key.sourceGeneration == _previewSourceGeneration && _readingSource &&
               frame.math.key.textGeneration == _readingSource->textGeneration;
    }

    void MarkdownView::RetireMath() noexcept
    {
        try
        {
            if (_derivedLayoutRefreshTimer && _owningDispatcher && _owningDispatcher.HasThreadAccess())
                _derivedLayoutRefreshTimer.Stop();
        }
        catch (...)
        {
        }
        if (_mathCoordinator)
            _mathCoordinator->CancelAll();
        _mathPresentations = {};
        _derivedEditReuse.reset();
        _mathBound = false;
        _derivedLayoutDeviceRecovery = false;
        _offscreenDerivedPendingAt.reset();
        ++_mathAttempt;
        ++_mathGeneration;
    }

    uint32_t MarkdownView::DerivedResidentAllowance(Native::ReadingObjectKind family) const noexcept
    {
        // Reserve at most half the shared control for the other enabled family,
        // donating every unused slot. For actual demands m,d and capacity c:
        // min(m,c-min(d,c/2))+min(d,c-min(m,c/2)) <= c.
        constexpr uint32_t capacity = NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT;
        static_assert(capacity > 0 && capacity % 2 == 0);
        constexpr uint32_t half = capacity / 2;
        auto const other = family == Native::ReadingObjectKind::Math
                               ? Native::ReadingObjectKind::Diagram
                               : Native::ReadingObjectKind::Math;
        auto const extension = other == Native::ReadingObjectKind::Math
                                   ? NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH : NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID;
        auto descriptors = _readingSource ? _readingSource->Descriptors() : nullptr;
        if (!descriptors ||
            (static_cast<uint32_t>(_acceptedExtensions) & extension) == 0)
            return capacity;
        uint32_t reserved{};
        for (auto const &object : descriptors->objects)
            if (object.kind == other && ++reserved == half)
                break;
        return capacity - reserved;
    }

    namespace
    {
        template <typename Table, typename Key, typename CanReuse>
        void RebindDerived(Native::SemanticDescriptors const &before, Native::SemanticDescriptors const &after,
                           Native::ReadingObjectKind kind, Table const &previous, Table &next,
                           Key const &key, uint32_t allowance, CanReuse canReuse) noexcept
        {
            std::array<Native::ReadingObjectDescriptor const *, NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT> inputs{};
            uint32_t priorOrdinal{};
            for (auto const &object : before.objects)
            {
                if (object.kind != kind)
                    continue;
                if (priorOrdinal++ == inputs.size())
                    break;
                for (size_t i = 0; i < previous.count; ++i)
                    if (previous.entries[i].object == object.object &&
                        canReuse(*previous.entries[i].handle.operator->()))
                        inputs[i] = &object;
            }

            uint32_t ordinal{};
            for (auto const &object : after.objects)
            {
                if (object.kind != kind)
                    continue;
                if (ordinal++ >= allowance)
                    break;
                for (size_t i = 0; i < previous.count; ++i)
                {
                    auto const *prior = inputs[i];
                    if (prior && prior->syntax == object.syntax && prior->input == object.input)
                    {
                        // Each occurrence retains its own funded presentation,
                        // including the metadata owner for a deterministic error.
                        next.Add(key, object.object, previous.entries[i].handle);
                        inputs[i] = nullptr;
                        break;
                    }
                }
            }
        }
    }

    std::optional<MarkdownView::DerivedEditReuse> MarkdownView::TakeDerivedEditReuse() noexcept
    {
        if (_readingSource && (_mathPresentations.count || _diagramPresentations.count))
            if (auto descriptors = _readingSource->Descriptors())
                return DerivedEditReuse{
                    std::move(descriptors), std::move(_mathPresentations), std::move(_diagramPresentations)};
        // No new artifact was installed during a batch superseded by more typing.
        return std::move(_derivedEditReuse);
    }

    bool MarkdownView::RestoreDerivedForEdit(Native::PreviewFrame const &frame) noexcept
    {
        auto previous = std::exchange(_derivedEditReuse, {});
        if (!previous || !frame.descriptors)
            return false;
        try
        {
            auto math = CaptureMathKey();
            auto diagram = CaptureDiagramKey();
            if (previous->math.count && frame.math.enabled)
                RebindDerived(*previous->descriptors, *frame.descriptors, Native::ReadingObjectKind::Math,
                    previous->math, _mathPresentations, math, DerivedResidentAllowance(Native::ReadingObjectKind::Math),
                    [&](Native::NativeMathPresentation const &presentation)
                    {
                        // Worker, memory, and placement failures retain retry semantics.
                        const auto &old = presentation.Key();
                        return (presentation.State() == Native::NativeMathState::Ready ||
                                presentation.Status() == NOTEPADS_MARKDOWN_MATH_PARSE_FAILURE) &&
                               old.factoryEpoch == math.factoryEpoch && old.engine == math.engine &&
                               old.fonts == math.fonts && old.style == math.style;
                    });
            if (previous->diagrams.count && frame.diagram.enabled)
                RebindDerived(*previous->descriptors, *frame.descriptors, Native::ReadingObjectKind::Diagram,
                    previous->diagrams, _diagramPresentations, diagram, DerivedResidentAllowance(Native::ReadingObjectKind::Diagram),
                    [&](Native::NativeDiagramPresentation const &presentation)
                    {
                        const auto &old = presentation.Key();
                        return (presentation.State() == Native::NativeDiagramState::Ready ||
                                presentation.Status() == NOTEPADS_MARKDOWN_DIAGRAM_INVALID_INPUT) &&
                               old.factoryEpoch == diagram.factoryEpoch && old.fontEpoch == diagram.fontEpoch &&
                               old.styleEpoch == diagram.styleEpoch && old.deviceEpoch == diagram.deviceEpoch &&
                               old.targetWidth == diagram.targetWidth && old.dpi == diagram.dpi &&
                               old.extensions == diagram.extensions;
                    });
            if (_mathPresentations.count)
                ++_mathGeneration;
            if (_diagramPresentations.count)
            {
                _diagramBoundKey = diagram;
                ++_diagramGeneration;
            }
            return _mathPresentations.count || _diagramPresentations.count;
        }
        catch (...)
        {
            // A failed configuration capture grants no reuse.
            return false;
        }
    }

    void MarkdownView::BindMath() noexcept
    {
        auto main = _readingSource ? _readingSource->Main() : nullptr;
        auto descriptors = _readingSource ? _readingSource->Descriptors() : nullptr;
        if (_resourceRenderingDeferred || _mathBound || !_loaded || !_documentAccepted || (static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH) == 0 ||
            !main || !descriptors)
            return;
        try
        {
            if (std::any_of(descriptors->objects.begin(), descriptors->objects.end(),
                            [](auto const &object) { return object.kind == Native::ReadingObjectKind::Math; }))
                CaptureImageAnchorBeforeChange();
            EnsureGraphicsFactory();
            EnsureDerivedControl();
            _mathCoordinator->BeginSource(_mathAttempt);
            _mathBound = true;
            auto key = CaptureMathKey();
            auto const residentLimit = DerivedResidentAllowance(Native::ReadingObjectKind::Math);
            uint32_t ordinal{};
            bool requested{};
            for (auto const &object : descriptors->objects)
            {
                if (object.kind != Native::ReadingObjectKind::Math)
                    continue;
                if (ordinal == NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
                    break;
                auto const index = ordinal++;
                if (_mathPresentations.Find(object.object))
                    continue;
                requested = true;
                key.object = object.object;
                key.syntax = object.syntax;
                notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
                (void)_mathCoordinator->Request(main->ModelOwner().Get(), key, index,
                                                _d2dFactory.get(), &error, residentLimit);
            }
            if (requested)
            {
                ++_mathGeneration;
                QueueDerivedLayoutRefresh();
            }
        }
        catch (...)
        {
            // No ordinary layout event retries allocation indefinitely. An
            // explicit Refresh/new source opens the next attempt scope.
            _mathBound = true;
            ++_mathGeneration;
            try
            {
                QueueDerivedLayoutRefresh();
            }
            catch (...)
            {
            }
        }
    }

    void MarkdownView::AcceptMathDelivery(Windows::Foundation::IInspectable const &target,
                                          Native::MathPresentationHandle const &payload) noexcept
    {
        MarkdownView *owner{};
        bool currentDelivery{};
        try
        {
            owner = get_self<MarkdownView>(target.as<Markdown::MarkdownView>());
            if (!payload || !owner->_loaded || !owner->_documentAccepted || !owner->_mathBound ||
                (static_cast<uint32_t>(owner->_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH) == 0 || !owner->_readingSource)
                return;
            auto const &key = payload->Key();
            const auto source = owner->CaptureMathKey();
            auto current = source;
            current.object = key.object;
            current.syntax = key.syntax;
            if (!(current == key) || owner->_mathPresentations.Find(key.object))
                return;
            currentDelivery = true;
            // Preserve the last coherent viewport before this delivery makes
            // its resource generation unavailable to navigation again.
            owner->CaptureImageAnchorBeforeChange();
            if (owner->_mathPresentations.count < owner->_mathPresentations.entries.size() && payload->LinkBefore({}))
                owner->_mathPresentations.Add(source, key.object, payload);
        }
        catch (...)
        {
            // Transport drops its actual owner after this callback returns.
            // A dead/obsolete XAML target never acquires presentation ownership.
        }
        if (currentDelivery)
        {
            // A rejected link/capture is terminal too. Publish the receipt's
            // settled attempt state on the next UI batch, rather than retaining
            // an old Pending frame and its image anchor indefinitely.
            ++owner->_mathGeneration;
            try { owner->QueueDerivedLayoutRefresh(); }
            catch (...) { /* A later normal layout observes the settled state. */ }
        }
    }

    void MarkdownView::QueueDerivedLayoutRefresh()
    {
        if (!_loaded || !_readingSource)
            return;
        if (!_derivedLayoutRefreshTimer)
        {
            _derivedLayoutRefreshTimer = Windows::UI::Xaml::DispatcherTimer{};
            _derivedLayoutRefreshTimer.Interval(std::chrono::milliseconds(16));
            _derivedLayoutRefreshTickRevoker =
                _derivedLayoutRefreshTimer.Tick(auto_revoke,
                                       [weak = get_weak()](auto const &, auto const &)
                                       {
                                           if (auto owner = weak.get())
                                           {
                                               owner->_derivedLayoutRefreshTimer.Stop();
                                               if (owner->_loaded && owner->_documentAccepted && owner->_readingSource)
                                                   owner->RefreshDerivedLayout();
                                           }
                                       });
        }
        if (!_derivedLayoutRefreshTimer.IsEnabled())
            _derivedLayoutRefreshTimer.Start();
    }

    hstring MarkdownView::ReadingMathDescription(uint32_t object, uint32_t syntax) const
    {
        auto frame = _presented.frame;
        auto state = frame ? frame->MathState(object) : Native::NativeMathState::Unavailable;
        if (frame && frame->math.enabled &&
            (state == Native::NativeMathState::Unsupported || state == Native::NativeMathState::Failed))
        {
            // Only the published frame supplies the epoch/attempt identity.
            // Keep final placement/resource, loading and cancellation states authoritative.
            if (auto const *presentation = frame->math.presentations.Find(frame->math.key, object);
                presentation && (*presentation)->Key().syntax == syntax)
            {
                switch ((*presentation)->Status())
                {
                case NOTEPADS_MARKDOWN_MATH_MISSING_FONT:
                    return L"Formula font is unavailable. Copy exports the complete original source.";
                case NOTEPADS_MARKDOWN_MATH_MISSING_GLYPH:
                    return L"Formula contains a character missing from its font. Copy exports the complete original source.";
                case NOTEPADS_MARKDOWN_MATH_PARSE_FAILURE:
                    return L"Formula syntax error. Copy exports the complete original source.";
                default:
                    break;
                }
            }
        }
        switch (state)
        {
        case Native::NativeMathState::Ready:
            return L"Formula rendered. Copy exports the complete original source.";
        case Native::NativeMathState::Pending:
        case Native::NativeMathState::None:
            return L"Formula loading. Copy exports the complete original source.";
        case Native::NativeMathState::Limited:
            return L"Formula resource or placement limit. Copy exports the complete original source.";
        case Native::NativeMathState::Unsupported:
            return L"Formula is not supported. Copy exports the complete original source.";
        case Native::NativeMathState::Cancelled:
            return L"Formula cancelled. Copy exports the complete original source.";
        case Native::NativeMathState::Failed:
            return L"Formula rendering failed. Copy exports the complete original source.";
        default:
            return L"Formula rendering unavailable. Copy exports the complete original source.";
        }
    }
}
