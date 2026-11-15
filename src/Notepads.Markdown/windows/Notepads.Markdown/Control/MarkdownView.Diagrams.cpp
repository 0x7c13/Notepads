// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Automation/ReadingObjectProvider.h"
#include <dwrite_2.h>
#include <algorithm>
#include <cmath>

namespace winrt::Notepads::Markdown::implementation
{
    namespace Native = ::Notepads::Markdown::Native;

    void MarkdownView::EnsureDerivedControl()
    {
        if (_mathCoordinator)
            return;
        Native::NativeMathDeliverySink sink;
        sink.target = make_weak(get_strong().as<Windows::Foundation::IInspectable>());
        sink.dispatcher = Dispatcher();
        sink.invoke = &MarkdownView::AcceptMathDelivery;
        _mathCoordinator = Native::NativeMathCoordinator::Create(sink);
    }

    void MarkdownView::EnsureDiagramFonts()
    {
        CheckUiThread();
        if (!_writeFactory)
            check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                                              reinterpret_cast<IUnknown **>(_writeFactory.put())));
        if (_diagramFontCollection && _diagramFontFallback)
            return;
        com_ptr<IDWriteFontCollection> collection;
        check_hresult(_writeFactory->GetSystemFontCollection(collection.put(), FALSE));
        auto factory2 = _writeFactory.as<IDWriteFactory2>();
        com_ptr<IDWriteFontFallbackBuilder> builder;
        check_hresult(factory2->CreateFontFallbackBuilder(builder.put()));
        DWRITE_UNICODE_RANGE range{0, 0x10ffff};
        wchar_t const *families[]{L"Segoe UI", L"Microsoft YaHei UI", L"Segoe UI Symbol", L"Segoe UI Emoji"};
        check_hresult(builder->AddMapping(&range, 1, families, 4, collection.get(), nullptr, nullptr, 1.0f));
        com_ptr<IDWriteFontFallback> fallback;
        check_hresult(builder->CreateFontFallback(fallback.put()));
        std::array<uint16_t, 85> locale{};
        if (!GetUserDefaultLocaleName(reinterpret_cast<wchar_t *>(locale.data()), static_cast<int>(locale.size())))
            throw_last_error();
        _diagramFontCollection = std::move(collection);
        _diagramFontFallback = std::move(fallback);
        _diagramLocale = locale;
    }

    Native::NativeDiagramKey MarkdownView::CaptureDiagramKey() const
    {
        Native::NativeDiagramKey key;
        if ((static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID) == 0 || !_readingSource)
            return key;
        key.identity = _readingSource->memory->Identity();
        key.sourceGeneration = _previewSourceGeneration;
        key.textGeneration = _readingSource->textGeneration;
        key.attempt = _diagramAttempt;
        key.factoryEpoch = _mathFactoryEpoch;
        key.fontEpoch = _diagramFontEpoch;
        key.styleEpoch = _effectiveThemeGeneration;
        key.dpiEpoch = _diagramAttempt;
        key.targetWidthEpoch = _diagramAttempt;
        key.deviceEpoch = _mathFactoryEpoch;
        key.extensions = static_cast<uint32_t>(_acceptedExtensions);
        auto width =
            _previewScroll && _previewScroll.ViewportWidth() > 0 ? _previewScroll.ViewportWidth() : ActualWidth();
        key.targetWidth = (std::max)(1.0, width - 2 * _padding);
        // Match the existing preview layout's quarter-DIP resize threshold.
        // A sub-threshold size event keeps the same effective layout width.
        if ((_diagramBound || _diagramPresentations.count) && std::abs(key.targetWidth - _diagramBoundKey.targetWidth) <= 0.25)
            key.targetWidth = _diagramBoundKey.targetWidth;
        key.dpi = 96.0 * EffectiveRasterScale();
        return key;
    }

    Native::PreviewDiagramSnapshot MarkdownView::CaptureDiagram() const
    {
        Native::PreviewDiagramSnapshot snapshot;
        snapshot.enabled = (static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID) != 0;
        snapshot.presentations = _diagramPresentations;
        snapshot.generation = _diagramGeneration;
        if (!snapshot.enabled || !_readingSource)
            return snapshot;
        snapshot.key = CaptureDiagramKey();
        if (_diagramBound && _diagramCoordinator)
            snapshot.attempts = _diagramCoordinator->AttemptStates(_diagramAttempt);
        else if (_diagramBound)
            snapshot.attempts.fill(Native::NativeDiagramState::Unavailable);
        return snapshot;
    }

    notepads_markdown_diagram_style MarkdownView::CaptureDiagramStyle() const
    {
        notepads_markdown_diagram_style style{{NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION, sizeof(notepads_markdown_diagram_style)}};
        auto channels = [](uint8_t (&out)[4], Windows::UI::Color const &color) noexcept
        {
            out[0] = color.R;
            out[1] = color.G;
            out[2] = color.B;
            out[3] = color.A;
        };
        channels(style.foreground, _foreground);
        channels(style.background, _background);
        bool highContrast = _accessibility && _accessibility.HighContrast();
        channels(style.border, highContrast ? _foreground : _readingTheme.borderColor);
        channels(style.panel, highContrast ? _background : _readingTheme.panelBackground);
        channels(style.link, highContrast ? _foreground : _readingTheme.linkForeground);
        style.dark = (_background.R + _background.G + _background.B) < 384;
        return style;
    }

    bool MarkdownView::CurrentDiagramFrame(Native::PreviewFrame const &frame) const noexcept
    {
        try
        {
            const bool enabled =
                (static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID) != 0;
            return frame.diagram.enabled == enabled &&
                   (!enabled || (frame.diagram.generation <= _diagramGeneration && frame.diagram.key == CaptureDiagramKey()));
        }
        catch (...)
        {
            return false;
        }
    }

    void MarkdownView::InvalidateDiagramConfiguration()
    {
        if (!_diagramBound && !_diagramPresentations.count)
            return;
        auto current = CaptureDiagramKey();
        if (!(current == _diagramBoundKey))
            RetireDiagram();
    }

    void MarkdownView::RetireDiagram() noexcept
    {
        _diagramInspection.reset();
        _pendingDiagramInspection.reset();
        if (_diagramCoordinator)
            _diagramCoordinator->CancelAll();
        _diagramPresentations = {};
        _derivedEditReuse.reset();
        _diagramBound = false;
        _diagramBoundKey = {};
        ++_diagramAttempt;
        ++_diagramGeneration;
    }

    void MarkdownView::BindDiagram() noexcept
    {
        auto main = _readingSource ? _readingSource->Main() : nullptr;
        auto descriptors = _readingSource ? _readingSource->Descriptors() : nullptr;
        if (_resourceRenderingDeferred || _diagramBound || !_loaded || !_documentAccepted ||
            (static_cast<uint32_t>(_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID) == 0 || !_readingSource ||
            !main || !descriptors)
            return;
        try
        {
            if (std::any_of(descriptors->objects.begin(), descriptors->objects.end(),
                            [](auto const &object) { return object.kind == Native::ReadingObjectKind::Diagram; }))
                CaptureImageAnchorBeforeChange();
            EnsureGraphicsFactory();
            EnsureDerivedControl();
            EnsureDiagramFonts();
            Native::NativeDiagramDeliverySink sink;
            sink.target = make_weak(get_strong().as<Windows::Foundation::IInspectable>());
            sink.dispatcher = Dispatcher();
            sink.invoke = &MarkdownView::AcceptDiagramDelivery;
            auto model = main->ModelOwner().Get();
            if (!_diagramCoordinator)
                _diagramCoordinator = Native::NativeDiagramCoordinator::Create(_mathCoordinator, model, sink);
            _diagramCoordinator->BeginSource(_diagramAttempt);
            auto key = CaptureDiagramKey();
            _diagramBoundKey = key;
            _diagramBound = true;
            notepads_markdown_diagram_font_capability fonts{};
            fonts.version = 1;
            fonts.size = sizeof(fonts);
            fonts.factory = _writeFactory.get();
            fonts.collection = _diagramFontCollection.get();
            fonts.fallback = _diagramFontFallback.get();
            fonts.font_epoch = _diagramFontEpoch;
            std::copy(_diagramLocale.begin(), _diagramLocale.end(), fonts.locale_name);
            auto style = CaptureDiagramStyle();
            auto const residentLimit = DerivedResidentAllowance(Native::ReadingObjectKind::Diagram);
            uint32_t ordinal{};
            bool requested{};
            for (auto const &object : descriptors->objects)
            {
                if (object.kind != Native::ReadingObjectKind::Diagram)
                    continue;
                if (ordinal == NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT)
                    break;
                auto const index = ordinal++;
                if (_diagramPresentations.Find(object.object))
                    continue;
                requested = true;
                key.object = object.object;
                notepads_markdown_core_error error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
                (void)_diagramCoordinator->Request(model, key, index, fonts, style, &error, residentLimit);
            }
            if (requested)
            {
                ++_diagramGeneration;
                QueueDerivedLayoutRefresh();
            }
        }
        catch (...)
        {
            _diagramBound = true;
            ++_diagramGeneration;
            try
            {
                _diagramBoundKey = CaptureDiagramKey();
                QueueDerivedLayoutRefresh();
            }
            catch (...)
            {
            }
        }
    }

    void MarkdownView::AcceptDiagramDelivery(Windows::Foundation::IInspectable const &target,
                                             Native::DiagramPresentationHandle const &payload) noexcept
    {
        MarkdownView *owner{};
        bool currentDelivery{};
        try
        {
            owner = get_self<MarkdownView>(target.as<Markdown::MarkdownView>());
            if (!payload || !owner->_loaded || !owner->_documentAccepted || !owner->_diagramBound ||
                (static_cast<uint32_t>(owner->_acceptedExtensions) & NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID) == 0 ||
                !owner->_readingSource)
                return;
            const auto source = owner->CaptureDiagramKey();
            auto current = source;
            auto const &key = payload->Key();
            current.object = key.object;
            if (!(current == key) || owner->_diagramPresentations.Find(key.object))
                return;
            currentDelivery = true;
            owner->CaptureImageAnchorBeforeChange();
            if (owner->_diagramPresentations.count < owner->_diagramPresentations.entries.size() && payload->LinkBefore({}))
                owner->_diagramPresentations.Add(source, key.object, payload);
        }
        catch (...)
        {
        }
        if (currentDelivery)
        {
            ++owner->_diagramGeneration;
            try { owner->QueueDerivedLayoutRefresh(); }
            catch (...) { /* A later normal layout observes the settled state. */ }
        }
    }

    Native::PreviewDiagramVisual const *MarkdownView::FindDiagramVisual(
        Native::PreviewFrame const &frame, Native::NativeDiagramKey const &key, bool requireVisible) noexcept
    {
        auto current = frame.diagram.key;
        current.object = key.object;
        if (!(current == key) || !frame.measuredBlocks || frame.DiagramState(key.object) != Native::NativeDiagramState::Ready)
            return nullptr;
        for (auto const &block : frame.Blocks())
            for (auto const &text : block.texts)
                for (auto const &diagram : text.diagrams)
                    if (diagram.object == key.object && diagram.presentation)
                    {
                        auto const top = block.block.y - frame.viewportY + text.y + diagram.y;
                        if (!requireVisible || (top < frame.viewportHeight && top + diagram.height > 0))
                            return &diagram;
                    }
        return nullptr;
    }

    namespace
    {
        D2D1_RECT_F InspectionClip(float width, float height) noexcept
        {
            auto const margin = (std::min)({8.0f, width / 8.0f, height / 8.0f});
            return D2D1::RectF(margin, margin, width - margin, height - margin);
        }
    }

    bool MarkdownView::ReadingDiagramEnabled(TextSnapshot const &snapshot, size_t index) const
    {
        CheckLiveRange(snapshot);
        if (!ReadingAvailable() || !snapshot.descriptors || index >= snapshot.descriptors->objects.size())
            return false;
        auto const &object = snapshot.descriptors->objects[index];
        return object.kind == Native::ReadingObjectKind::Diagram &&
               ReadingChildVisible(snapshot, object.text) &&
               _presented.frame->DiagramState(object.object) == Native::NativeDiagramState::Ready;
    }

    bool MarkdownView::ReadingDiagramFocused(TextSnapshot const &snapshot, size_t index) const
    {
        return ReadingDiagramEnabled(snapshot, index) && _readingFocused &&
            _focusedAction.kind == Native::ReadingActionKind::Diagram && _focusedAction.index == index;
    }

    void MarkdownView::FocusReadingDiagram(TextSnapshot const &snapshot, size_t index)
    {
        if (index > UINT32_MAX)
            throw hresult_invalid_argument();
        FocusReadingAction(snapshot, {Native::ReadingActionKind::Diagram, static_cast<uint32_t>(index)});
    }

    void MarkdownView::InvokeReadingDiagram(TextSnapshot const &snapshot, size_t index)
    {
        auto lifetime = get_strong();
        if (!ReadingDiagramEnabled(snapshot, index))
            throw hresult_error(UIA_E_ELEMENTNOTENABLED);
        const auto object = snapshot.descriptors->objects[index].object;
        if (_diagramInspection && _diagramInspection->key.object == object)
        {
            _diagramInspection.reset();
            DrawPreview();
            return;
        }
        FocusReadingDiagram(snapshot, index);
        if (!ReadingDiagramEnabled(snapshot, index) || _focusedAction.kind != Native::ReadingActionKind::Diagram ||
            _focusedAction.index != index)
            return;

        auto key = CaptureDiagramKey();
        key.object = object;
        ClearReadingHover();
        _readingLinkPoint.reset();
        _codeCopyPointer.reset();
        SetReadingLinkCursor(false);
        if (!FindDiagramVisual(*_presented.frame, key, true))
        {
            _pendingDiagramInspection = key;
            StartPreviewFrame();
            return;
        }
        _diagramInspection = DiagramInspection{key, {}};
        DrawPreview();
    }

    std::optional<MarkdownView::ReadingDiagramInspection> MarkdownView::DiagramInspectionInfo(
        TextSnapshot const &snapshot, size_t index) const
    {
        if (!_presented.inspection || !ReadingDiagramEnabled(snapshot, index) ||
            snapshot.descriptors->objects[index].object != _presented.inspection->state.key.object)
            return std::nullopt;
        return ReadingDiagramInspection{_presented.inspection->state.transform, _presented.inspection->placement};
    }

    std::optional<Native::ReadingRect> MarkdownView::DiagramInspectionReadingRect(TextSnapshot const &snapshot) const
    {
        if (!_presented.inspection || !snapshot.descriptors)
            return std::nullopt;
        auto const object = _presented.inspection->state.key.object;
        auto const &objects = snapshot.descriptors->objects;
        auto at = std::find_if(objects.begin(), objects.end(), [object](auto const &descriptor)
        {
            return descriptor.kind == Native::ReadingObjectKind::Diagram && descriptor.object == object;
        });
        if (at == objects.end())
            return std::nullopt;
        auto const &clip = _presented.inspection->placement.clip;
        auto const span = at->text;
        return Native::ReadingRect{static_cast<float>(_presented.geometry.x) + clip.left,
            static_cast<float>(_presented.geometry.y) + clip.top, clip.right - clip.left, clip.bottom - clip.top,
            span.start, span.end};
    }

    void MarkdownView::SetReadingDiagramInspection(TextSnapshot const &snapshot, size_t index,
        Native::DiagramInspectionTransform const &transform)
    {
        if (!_diagramInspection || !DiagramInspectionInfo(snapshot, index))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        if (!std::isfinite(transform.zoom) || !std::isfinite(transform.x) || !std::isfinite(transform.y) ||
            transform.zoom < Native::DiagramInspectionTransform::MinimumZoom ||
            transform.zoom > Native::DiagramInspectionTransform::MaximumZoom ||
            transform.x < 0 || transform.x > 1 || transform.y < 0 || transform.y > 1)
            throw hresult_invalid_argument();

        if (transform != _diagramInspection->transform)
        {
            _diagramInspection->transform = transform;
            DrawPreview();
        }
    }

    bool MarkdownView::ApplyDiagramInspectionKey(Windows::System::VirtualKey key)
    {
        using Key = Windows::System::VirtualKey;
        if (key == Key::Escape || key == Key::Tab)
        {
            _diagramInspection.reset();
            DrawPreview();
            return key != Key::Tab;
        }
        if (!_automationSnapshot || _focusedAction.kind != Native::ReadingActionKind::Diagram)
            return false;
        auto const index = _focusedAction.index;
        auto view = DiagramInspectionInfo(*_automationSnapshot, index);
        if (!view)
            return false;

        auto transform = view->transform;
        auto const pan = [](float position, float range, float by)
        {
            return range > 0 ? std::clamp(position + by / range, 0.0f, 1.0f) : position;
        };
        if (static_cast<int>(key) == VK_OEM_PLUS)
            key = Key::Add;
        else if (static_cast<int>(key) == VK_OEM_MINUS)
            key = Key::Subtract;
        switch (key)
        {
        case Key::Home: transform = {}; break;
        case Key::Left: transform.x = pan(transform.x, view->placement.scrollWidth, -48); break;
        case Key::Right: transform.x = pan(transform.x, view->placement.scrollWidth, 48); break;
        case Key::Up: transform.y = pan(transform.y, view->placement.scrollHeight, -48); break;
        case Key::Down: transform.y = pan(transform.y, view->placement.scrollHeight, 48); break;
        case Key::Add:
            transform.zoom = (std::min)(transform.zoom * 1.25f, Native::DiagramInspectionTransform::MaximumZoom);
            break;
        case Key::Subtract:
            transform.zoom = (std::max)(transform.zoom / 1.25f, Native::DiagramInspectionTransform::MinimumZoom);
            break;
        default:
            return false;
        }
        SetReadingDiagramInspection(*_automationSnapshot, index, transform);
        return true;
    }

    bool MarkdownView::DrawDiagramInspection(DiagramInspection const &inspection, Native::PreviewFrame const &frame,
                                             ID2D1DeviceContext *context,
                                             Native::ViewportGeometry const &geometry,
                                             Native::DiagramInspectionPlacement &placement) noexcept
    {
        auto const visual = FindDiagramVisual(frame, inspection.key);
        if (!visual || !Native::NativeDiagramDrawing::Inspect(visual->presentation,
                InspectionClip(static_cast<float>(geometry.width), static_cast<float>(geometry.height)),
                inspection.transform, placement))
            return false;
        auto clip = placement.clip;
        auto phaseX = static_cast<float>(geometry.phaseX);
        auto phaseY = static_cast<float>(geometry.phaseY);
        clip.left += phaseX;
        clip.right += phaseX;
        clip.top += phaseY;
        clip.bottom += phaseY;
        return SUCCEEDED(Native::NativeDiagramDrawing::Draw(visual->presentation, placement.diagram, context,
            placement.origin.x + phaseX, placement.origin.y + phaseY, visual->drawing, &clip));
    }

    void MarkdownView::NotifyDiagramInspectionPublished(std::optional<PresentedDiagramInspection> const &previous) noexcept
    {
        try
        {
            using namespace Windows::UI::Xaml::Automation;
            auto snapshot = _automationSnapshot;
            auto const current = _presented.inspection;
            auto const publication = _presented.serial;
            auto const attachment = _previewSurfaceSerial;
            auto stillCurrent = [&]
            {
                return publication == _presented.serial && attachment == _previewSurfaceSerial &&
                    _automationSnapshot == snapshot && ReadingAvailable();
            };
            if ((!previous && !current) || !snapshot || !snapshot->descriptors || !stillCurrent())
                return;
            if (previous.has_value() != current.has_value())
            {
                UpdateNoteEmbeds();
                if (!stillCurrent())
                    return;
            }
            auto notify = [&](Native::NativeDiagramKey const &key, PresentedDiagramInspection const *before,
                              PresentedDiagramInspection const *after)
            {
                if (!stillCurrent())
                    return;
                auto accepted = CaptureDiagramKey();
                accepted.object = key.object;
                if (!(accepted == key))
                    return;
                auto const &objects = snapshot->descriptors->objects;
                auto at = std::find_if(objects.begin(), objects.end(), [&](auto const &object)
                {
                    return object.kind == Native::ReadingObjectKind::Diagram && object.object == key.object;
                });
                if (at == objects.end())
                    return;
                auto peer = ReadingPeer(snapshot, Native::ReadingChildKind::Object,
                    static_cast<size_t>(at - objects.begin()));
                auto raise = [&](auto property, auto from, auto to)
                {
                    if (from != to && stillCurrent())
                        peer->RaisePropertyChangedEvent(property, box_value(from), box_value(to));
                };
                if (!before || !after)
                {
                    if (stillCurrent())
                        peer->InvalidatePeer();
                    raise(TransformPattern2Identifiers::CanZoomProperty(), before != nullptr, after != nullptr);
                    return;
                }
                auto const &left = before->placement;
                auto const &right = after->placement;
                raise(TransformPattern2Identifiers::ZoomLevelProperty(),
                      100.0 * before->state.transform.zoom, 100.0 * after->state.transform.zoom);
                raise(ScrollPatternIdentifiers::HorizontallyScrollableProperty(), left.scrollWidth > 0, right.scrollWidth > 0);
                raise(ScrollPatternIdentifiers::VerticallyScrollableProperty(), left.scrollHeight > 0, right.scrollHeight > 0);
                raise(ScrollPatternIdentifiers::HorizontalScrollPercentProperty(),
                      left.scrollWidth > 0 ? 100.0 * before->state.transform.x : -1.0,
                      right.scrollWidth > 0 ? 100.0 * after->state.transform.x : -1.0);
                raise(ScrollPatternIdentifiers::VerticalScrollPercentProperty(),
                      left.scrollHeight > 0 ? 100.0 * before->state.transform.y : -1.0,
                      right.scrollHeight > 0 ? 100.0 * after->state.transform.y : -1.0);
                raise(ScrollPatternIdentifiers::HorizontalViewSizeProperty(),
                      (std::min)(100.0, 100.0 * (left.clip.right - left.clip.left) / left.diagram.width),
                      (std::min)(100.0, 100.0 * (right.clip.right - right.clip.left) / right.diagram.width));
                raise(ScrollPatternIdentifiers::VerticalViewSizeProperty(),
                      (std::min)(100.0, 100.0 * (left.clip.bottom - left.clip.top) / left.diagram.height),
                      (std::min)(100.0, 100.0 * (right.clip.bottom - right.clip.top) / right.diagram.height));
            };
            if (previous && current && previous->state.key == current->state.key)
            {
                if (previous->state != current->state ||
                    previous->placement.clip.right != current->placement.clip.right ||
                    previous->placement.clip.bottom != current->placement.clip.bottom)
                    notify(current->state.key, &*previous, &*current);
            }
            else
            {
                if (previous)
                    notify(previous->state.key, &*previous, nullptr);
                if (current)
                    notify(current->state.key, nullptr, &*current);
            }
        }
        catch (...)
        {
            // UIA clients never own viewport publication.
        }
    }

    hstring MarkdownView::ReadingDiagramDescription(uint32_t object) const
    {
        auto state = _presented.frame ? _presented.frame->DiagramState(object) : Native::NativeDiagramState::Unavailable;
        switch (state)
        {
        case Native::NativeDiagramState::Ready:
            return L"Diagram rendered. Activate to inspect. Copy exports the complete original source.";
        case Native::NativeDiagramState::Pending:
        case Native::NativeDiagramState::None:
            return L"Diagram loading. Copy exports the complete original source.";
        case Native::NativeDiagramState::Unsupported:
            return L"Diagram feature unsupported. Copy exports the complete original source.";
        case Native::NativeDiagramState::Limited:
            return L"Diagram resource or placement limit. Copy exports the complete original source.";
        case Native::NativeDiagramState::FontUnavailable:
            return L"Diagram font unavailable. Copy exports the complete original source.";
        case Native::NativeDiagramState::Cancelled:
            return L"Diagram cancelled. Copy exports the complete original source.";
        case Native::NativeDiagramState::Failed:
            return L"Diagram rendering failed. Copy exports the complete original source.";
        default:
            return L"Diagram rendering unavailable. Copy exports the complete original source.";
        }
    }
}
