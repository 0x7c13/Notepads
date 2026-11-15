// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Automation/ReadingObjectProvider.h"
#include "ReadingObjectProvider.g.cpp"
#include "Automation/TextRangeProvider.h"
#include "notepads/markdown/images.h"
#include <algorithm>
#include <cmath>

namespace winrt::Notepads::Markdown::implementation
{
    using namespace Windows::Foundation;
    using namespace Windows::UI::Xaml::Automation::Peers;
    using ::Notepads::Markdown::Native::ReadingChildKind;
    using ::Notepads::Markdown::Native::ReadingObjectKind;
    using ::Notepads::Markdown::Native::DiagramInspectionTransform;
    using Windows::UI::Xaml::Automation::ScrollAmount;
    using Windows::UI::Xaml::Automation::ZoomUnit;
    ReadingObjectProvider::ReadingObjectProvider(weak_ref<MarkdownView> owner,
                                                 std::shared_ptr<TextSnapshot const> snapshot,
                                                 ReadingChildKind kind, size_t index)
    {
        if (!snapshot || !snapshot->main || !snapshot->descriptors)
            throw hresult_invalid_argument();
        auto const &descriptors = *snapshot->descriptors;
        auto span = descriptors.Child(kind, index);
        _state =
            std::allocate_shared<State>(::Notepads::Markdown::Native::ReadingAllocator<State>{snapshot->main->Memory()},
                                        State{std::move(owner), snapshot, kind, index, span});
        if (kind == ReadingChildKind::Object && descriptors.objects[index].kind == ReadingObjectKind::Math)
        {
            auto const &object = descriptors.objects[index];
            _mathHelpText = Owner()->ReadingMathDescription(object.object, object.syntax);
        }
    }
    void ReadingObjectProvider::UpdateMathHelpText(hstring value)
    {
        if (_mathHelpText == value)
            return;
        auto previous = std::exchange(_mathHelpText, std::move(value));
        using Windows::UI::Xaml::Automation::AutomationElementIdentifiers;
        RaisePropertyChangedEvent(AutomationElementIdentifiers::HelpTextProperty(), box_value(previous), box_value(_mathHelpText));
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        ++_mathHelpTextChanges;
#endif
    }
    com_ptr<MarkdownView> ReadingObjectProvider::Owner() const
    {
        auto owner = _state->owner.get();
        if (!owner)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        if (!owner->ReadingChildVisible(*_state->snapshot, _state->span))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return owner;
    }
    ReadingObjectProvider::Provider ReadingObjectProvider::Raw()
    {
        auto lifetime = get_strong();
        return ProviderFromPeer(*this);
    }
    hstring ReadingObjectProvider::GetClassNameCore()
    {
        return L"Notepads.Markdown.ReadingObject";
    }
    hstring ReadingObjectProvider::GetNameCore()
    {
        auto lifetime = get_strong();
        auto owned = _state;
        auto const &state = *owned;
        auto owner = Owner();
        if (state.kind == ReadingChildKind::Table || state.kind == ReadingChildKind::Caption)
        {
            auto const &descriptors = *state.snapshot->descriptors;
            auto caption = descriptors.tables[state.index].caption;
            auto label = descriptors.Copy(*state.snapshot->main, caption.start, caption.end);
            return hstring{std::wstring_view{label.data(), label.size()}};
        }
        if (state.kind == ReadingChildKind::Disclosure)
        {
            auto label = state.snapshot->descriptors->Copy(*state.snapshot->main, state.span.start, state.span.end);
            return label.empty() ? hstring{L"Details"} : hstring{std::wstring_view(label.data(), label.size())};
        }
        if (state.kind == ReadingChildKind::Link)
        {
            auto const &link = state.snapshot->descriptors->links[state.index];
            if (link.localPosition || link.localHeadingNode)
                return hstring{std::wstring_view(link.title.data(), link.title.size())};
            auto label = state.snapshot->descriptors->Copy(*state.snapshot->main, state.span.start, state.span.end);
            auto hasText = [](std::wstring_view text)
            {
                return std::any_of(text.begin(), text.end(), [](wchar_t value)
                                   { return value > L' ' && value != L'\x85' && value != L'\x2028' && value != L'\x2029'; });
            };
            if (!hasText(label))
            {
                return hstring{std::wstring_view(hasText(link.title) ? link.title : link.destination)};
            }
            return hstring{std::wstring_view(label.data(), label.size())};
        }
        auto const &object = state.snapshot->descriptors->objects[state.index];
        if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Task)
        {
            if (object.staticHtmlCheckbox)
            {
                return hstring{std::wstring_view(object.label.data(), object.label.size())};
            }
            bool enabled = owner->ReadingTaskEnabled(*state.snapshot, state.index);
            return object.checked ? (enabled ? L"Checked task" : L"Checked task (read only)")
                                  : (enabled ? L"Unchecked task" : L"Unchecked task (read only)");
        }
        if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Math ||
            object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Diagram)
        {
            auto const &displayName = object.title.empty() ? object.input : object.title;
            ::Notepads::Markdown::Native::ReadingString name{displayName};
            name.insert(0, object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Math ? L"Formula: " : L"Diagram: ");
            return hstring{std::wstring_view(name.data(), name.size())};
        }
        if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::NoteEmbed)
        {
            ::Notepads::Markdown::Native::ReadingString name{object.input};
            name.insert(0, L"Embedded note: ");
            return hstring{std::wstring_view(name.data(), name.size())};
        }
        auto imageState = owner->ReadingImageState(object.object);
        const bool loaded = imageState == NOTEPADS_MARKDOWN_IMAGE_READY;
        const bool loading = imageState == NOTEPADS_MARKDOWN_IMAGE_PENDING;
        if (object.alt.empty())
            return loaded ? L"Image, loaded" : loading ? L"Image, loading" : L"Image, not loaded";
        ::Notepads::Markdown::Native::ReadingString name{object.alt};
        name.append(loaded ? L", image loaded" : loading ? L", image loading" : L", image not loaded");
        return hstring{std::wstring_view(name.data(), name.size())};
    }
    hstring ReadingObjectProvider::GetHelpTextCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        (void)Owner();
        if (state->kind == ReadingChildKind::Disclosure || state->kind == ReadingChildKind::Table ||
            state->kind == ReadingChildKind::Caption)
            return {};
        if (state->kind == ReadingChildKind::Object)
        {
            auto const &object = state->snapshot->descriptors->objects[state->index];
            if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Image)
                return Owner()->ReadingImageDescription(object.object);
            if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Math)
                return Owner()->ReadingMathDescription(object.object, object.syntax);
            if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Diagram)
            {
                auto status = Owner()->ReadingDiagramDescription(object.object);
                if (object.label.empty())
                    return status;
                ::Notepads::Markdown::Native::ReadingString help{object.label.get_allocator()};
                help.assign(status.data(), status.size());
                help.push_back(L'\n');
                help.append(object.label);
                return hstring{std::wstring_view(help.data(), help.size())};
            }
            if (object.kind == ::Notepads::Markdown::Native::ReadingObjectKind::Task &&
                Owner()->ReadingTaskEnabled(*state->snapshot, state->index))
                return L"Requests a task state change from the document host";
            return L"Read only";
        }
        auto const &link = state->snapshot->descriptors->links[state->index];
        return hstring{std::wstring_view(link.destination.data(), link.destination.size())};
    }
    AutomationPeer ReadingObjectProvider::GetLabeledByCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        return state->kind == ReadingChildKind::Table
            ? Owner()->ReadingCaptionPeer(state->snapshot, state->index) : nullptr;
    }
    AutomationControlType ReadingObjectProvider::GetAutomationControlTypeCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind == ReadingChildKind::Link)
            return AutomationControlType::Hyperlink;
        if (state->kind == ReadingChildKind::Disclosure)
            return AutomationControlType::Button;
        if (state->kind == ReadingChildKind::Table)
            return AutomationControlType::Group;
        if (state->kind == ReadingChildKind::Caption)
            return AutomationControlType::Text;
        auto const &object = state->snapshot->descriptors->objects[state->index];
        auto kind = object.kind;
        if (kind == ::Notepads::Markdown::Native::ReadingObjectKind::Task &&
            (object.staticHtmlCheckbox || owner->ReadingTaskEnabled(*state->snapshot, state->index)))
            return AutomationControlType::CheckBox;
        return kind == ReadingObjectKind::Image || kind == ReadingObjectKind::Diagram ? AutomationControlType::Image
                                                                                      : AutomationControlType::Text;
    }
    IInspectable ReadingObjectProvider::GetPatternCore(PatternInterface pattern)
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (IsDiagram())
        {
            if (pattern == PatternInterface::Invoke && owner->ReadingDiagramEnabled(*state->snapshot, state->index))
                return *this;
            if ((pattern == PatternInterface::Scroll || pattern == PatternInterface::Transform ||
                 pattern == PatternInterface::Transform2) && owner->DiagramInspectionInfo(*state->snapshot, state->index))
                return *this;
        }
        if (pattern == PatternInterface::TextChild ||
            ((state->kind == ReadingChildKind::Link || state->kind == ReadingChildKind::Disclosure) &&
             pattern == PatternInterface::Invoke) ||
            (state->kind == ReadingChildKind::Disclosure && pattern == PatternInterface::ExpandCollapse) ||
            (state->kind == ReadingChildKind::Object && pattern == PatternInterface::Toggle &&
             (state->snapshot->descriptors->objects[state->index].staticHtmlCheckbox ||
              owner->ReadingTaskEnabled(*state->snapshot, state->index))))
            return *this;
        return nullptr;
    }
    Rect ReadingObjectProvider::GetBoundingRectangleCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind == ReadingChildKind::Table)
            return owner->ReadingTableRectangle(*state->snapshot, state->snapshot->descriptors->tables[state->index].node);
        auto rectangles = owner->ReadingRectangles(*state->snapshot, state->span.start, state->span.end);
        if (rectangles.empty())
            return {};
        auto result = owner->ReadingScreenRectangle(rectangles.front());
        for (size_t i = 1; i < rectangles.size(); ++i)
        {
            auto next = owner->ReadingScreenRectangle(rectangles[i]);
            auto right = std::max(result.X + result.Width, next.X + next.Width);
            auto bottom = std::max(result.Y + result.Height, next.Y + next.Height);
            result.X = std::min(result.X, next.X);
            result.Y = std::min(result.Y, next.Y);
            result.Width = right - result.X;
            result.Height = bottom - result.Y;
        }
        return result;
    }
    Point ReadingObjectProvider::GetClickablePointCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        Rect rect{};
        if (state->kind == ReadingChildKind::Table)
            rect = owner->ReadingTableRectangle(*state->snapshot, state->snapshot->descriptors->tables[state->index].node);
        else
        {
            auto rectangles = owner->ReadingRectangles(*state->snapshot, state->span.start, state->span.end);
            // The union can contain other text when a link spans multiple lines.
            auto visible = std::max_element(rectangles.begin(), rectangles.end(), [](auto const &left, auto const &right)
            {
                return left.width * left.height < right.width * right.height;
            });
            if (visible != rectangles.end())
                rect = owner->ReadingScreenRectangle(*visible);
        }
        if (rect.Width <= 0 || rect.Height <= 0)
            return {NAN, NAN};
        return {rect.X + rect.Width / 2, rect.Y + rect.Height / 2};
    }
    bool ReadingObjectProvider::IsOffscreenCore()
    {
        auto lifetime = get_strong();
        auto r = GetBoundingRectangleCore();
        return r.Width <= 0 || r.Height <= 0;
    }
    bool ReadingObjectProvider::IsEnabledCore()
    {
        auto lifetime = get_strong();
        Owner();
        auto state = _state;
        return state->kind != ReadingChildKind::Object || !state->snapshot->descriptors->objects[state->index].staticHtmlCheckbox;
    }
    bool ReadingObjectProvider::IsKeyboardFocusableCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        return state->kind == ReadingChildKind::Link || state->kind == ReadingChildKind::Disclosure ||
               (state->kind == ReadingChildKind::Object && (IsDiagram()
                   ? owner->ReadingDiagramEnabled(*state->snapshot, state->index)
                   : owner->ReadingTaskEnabled(*state->snapshot, state->index)));
    }
    bool ReadingObjectProvider::HasKeyboardFocusCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind == ReadingChildKind::Disclosure)
            return owner->ReadingDisclosureFocused(*state->snapshot, state->index);
        if (state->kind == ReadingChildKind::Link)
            return owner->ReadingLinkFocused(*state->snapshot, state->index);
        return state->kind == ReadingChildKind::Object && (IsDiagram()
            ? owner->ReadingDiagramFocused(*state->snapshot, state->index)
            : owner->ReadingTaskFocused(*state->snapshot, state->index));
    }
    bool ReadingObjectProvider::IsControlElementCore()
    {
        return true;
    }
    bool ReadingObjectProvider::IsContentElementCore()
    {
        return true;
    }
    void ReadingObjectProvider::SetFocusCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind == ReadingChildKind::Link)
        {
            owner->FocusReadingLink(*state->snapshot, state->index);
        }
        else if (state->kind == ReadingChildKind::Disclosure)
            owner->FocusReadingDisclosure(*state->snapshot, state->index);
        else if (IsDiagram())
            owner->FocusReadingDiagram(*state->snapshot, state->index);
        else if (state->kind == ReadingChildKind::Object && state->snapshot->descriptors->objects[state->index].kind ==
                 ::Notepads::Markdown::Native::ReadingObjectKind::Task)
            owner->FocusReadingTask(*state->snapshot, state->index);
    }
    Collections::IVector<AutomationPeer> ReadingObjectProvider::GetChildrenCore()
    {
        auto lifetime = get_strong();
        auto state = _state;
        if (state->kind == ReadingChildKind::Table || state->kind == ReadingChildKind::Caption)
            return Owner()->ReadingTableChildren(state->snapshot, state->index, state->kind == ReadingChildKind::Caption);
        return nullptr;
    }
    void ReadingObjectProvider::Invoke()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind == ReadingChildKind::Disclosure)
            owner->SetReadingDisclosure(*state->snapshot, state->index);
        else if (state->kind == ReadingChildKind::Link)
            owner->InvokeReadingLink(*state->snapshot, state->index);
        else if (IsDiagram())
            owner->InvokeReadingDiagram(*state->snapshot, state->index);
        else
            throw hresult_error(UIA_E_NOTSUPPORTED);
    }
    bool ReadingObjectProvider::IsDiagram() const noexcept
    {
        return _state->kind == ReadingChildKind::Object &&
               _state->snapshot->descriptors->objects[_state->index].kind == ReadingObjectKind::Diagram;
    }

    MarkdownView::ReadingDiagramInspection ReadingObjectProvider::RequireDiagramInspection(MarkdownView const &owner) const
    {
        if (IsDiagram())
            if (auto result = owner.DiagramInspectionInfo(*_state->snapshot, _state->index))
                return *result;
        throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
    }

    bool ReadingObjectProvider::HorizontallyScrollable()
    {
        return RequireDiagramInspection(*Owner()).placement.scrollWidth > 0;
    }

    bool ReadingObjectProvider::VerticallyScrollable()
    {
        return RequireDiagramInspection(*Owner()).placement.scrollHeight > 0;
    }

    double ReadingObjectProvider::HorizontalScrollPercent()
    {
        auto view = RequireDiagramInspection(*Owner());
        return view.placement.scrollWidth > 0 ? 100.0 * view.transform.x : -1;
    }

    double ReadingObjectProvider::VerticalScrollPercent()
    {
        auto view = RequireDiagramInspection(*Owner());
        return view.placement.scrollHeight > 0 ? 100.0 * view.transform.y : -1;
    }

    double ReadingObjectProvider::HorizontalViewSize()
    {
        auto view = RequireDiagramInspection(*Owner());
        return view.placement.scrollWidth > 0 ? 100.0 * (view.placement.clip.right - view.placement.clip.left) /
                                                view.placement.diagram.width : 100.0;
    }

    double ReadingObjectProvider::VerticalViewSize()
    {
        auto view = RequireDiagramInspection(*Owner());
        return view.placement.scrollHeight > 0 ? 100.0 * (view.placement.clip.bottom - view.placement.clip.top) /
                                                 view.placement.diagram.height : 100.0;
    }

    void ReadingObjectProvider::SetScrollPercent(double horizontal, double vertical)
    {
        auto lifetime = get_strong();
        auto owner = Owner();
        auto view = RequireDiagramInspection(*owner);
        auto set = [](double percent, float position, float range)
        {
            if (percent == -1)
                return position;
            if (!std::isfinite(percent) || percent < 0 || percent > 100)
                throw hresult_invalid_argument();
            if (range <= 0)
                throw hresult_error(UIA_E_INVALIDOPERATION);
            return static_cast<float>(percent / 100.0);
        };
        view.transform.x = set(horizontal, view.transform.x, view.placement.scrollWidth);
        view.transform.y = set(vertical, view.transform.y, view.placement.scrollHeight);
        owner->SetReadingDiagramInspection(*_state->snapshot, _state->index, view.transform);
    }

    void ReadingObjectProvider::Scroll(ScrollAmount const &horizontal, ScrollAmount const &vertical)
    {
        auto lifetime = get_strong();
        auto owner = Owner();
        auto view = RequireDiagramInspection(*owner);
        auto move = [](ScrollAmount amount, float position, float range, float size)
        {
            float delta{};
            switch (amount)
            {
            case ScrollAmount::NoAmount: return position;
            case ScrollAmount::LargeDecrement: delta = -0.9f * size; break;
            case ScrollAmount::SmallDecrement: delta = -48; break;
            case ScrollAmount::LargeIncrement: delta = 0.9f * size; break;
            case ScrollAmount::SmallIncrement: delta = 48; break;
            default: throw hresult_invalid_argument();
            }
            if (range <= 0)
                throw hresult_error(UIA_E_INVALIDOPERATION);
            return std::clamp(position + delta / range, 0.0f, 1.0f);
        };
        auto const &placement = view.placement;
        view.transform.x = move(horizontal, view.transform.x, placement.scrollWidth,
                                placement.clip.right - placement.clip.left);
        view.transform.y = move(vertical, view.transform.y, placement.scrollHeight,
                                placement.clip.bottom - placement.clip.top);
        owner->SetReadingDiagramInspection(*_state->snapshot, _state->index, view.transform);
    }

    bool ReadingObjectProvider::CanZoom()
    {
        auto owner = Owner();
        return IsDiagram() && owner->DiagramInspectionInfo(*_state->snapshot, _state->index).has_value();
    }

    double ReadingObjectProvider::ZoomLevel()
    {
        return 100.0 * RequireDiagramInspection(*Owner()).transform.zoom;
    }

    double ReadingObjectProvider::MinZoom()
    {
        (void)RequireDiagramInspection(*Owner());
        return 100.0 * DiagramInspectionTransform::MinimumZoom;
    }

    double ReadingObjectProvider::MaxZoom()
    {
        (void)RequireDiagramInspection(*Owner());
        return 100.0 * DiagramInspectionTransform::MaximumZoom;
    }

    void ReadingObjectProvider::Zoom(double zoom)
    {
        auto lifetime = get_strong();
        auto owner = Owner();
        auto view = RequireDiagramInspection(*owner);
        if (!std::isfinite(zoom) || zoom < MinZoom() || zoom > MaxZoom())
            throw hresult_invalid_argument();
        view.transform.zoom = static_cast<float>(zoom / 100.0);
        owner->SetReadingDiagramInspection(*_state->snapshot, _state->index, view.transform);
    }

    void ReadingObjectProvider::ZoomByUnit(ZoomUnit const &unit)
    {
        auto lifetime = get_strong();
        auto owner = Owner();
        auto view = RequireDiagramInspection(*owner);
        float factor;
        switch (unit)
        {
        case ZoomUnit::NoAmount: return;
        case ZoomUnit::LargeDecrement: factor = 0.5f; break;
        case ZoomUnit::SmallDecrement: factor = 0.8f; break;
        case ZoomUnit::LargeIncrement: factor = 2.0f; break;
        case ZoomUnit::SmallIncrement: factor = 1.25f; break;
        default: throw hresult_invalid_argument();
        }
        view.transform.zoom = std::clamp(view.transform.zoom * factor,
            DiagramInspectionTransform::MinimumZoom, DiagramInspectionTransform::MaximumZoom);
        owner->SetReadingDiagramInspection(*_state->snapshot, _state->index, view.transform);
    }

    void ReadingObjectProvider::Toggle()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind != ReadingChildKind::Object || !owner->ReadingTaskEnabled(*state->snapshot, state->index))
            throw hresult_error(UIA_E_ELEMENTNOTENABLED);
        owner->InvokeReadingTask(*state->snapshot, state->index);
    }
    Windows::UI::Xaml::Automation::ToggleState ReadingObjectProvider::ToggleState()
    {
        auto lifetime = get_strong();
        auto state = _state;
        auto owner = Owner();
        if (state->kind != ReadingChildKind::Object || (!state->snapshot->descriptors->objects[state->index].staticHtmlCheckbox &&
                           !owner->ReadingTaskEnabled(*state->snapshot, state->index)))
            throw hresult_error(UIA_E_ELEMENTNOTENABLED);
        return state->snapshot->descriptors->objects[state->index].checked
                   ? Windows::UI::Xaml::Automation::ToggleState::On
                   : Windows::UI::Xaml::Automation::ToggleState::Off;
    }
    ReadingObjectProvider::Provider ReadingObjectProvider::TextContainer()
    {
        auto lifetime = get_strong();
        auto owner = Owner();
        auto peer = FrameworkElementAutomationPeer::FromElement(owner.as<Markdown::MarkdownView>());
        if (!peer)
            peer = FrameworkElementAutomationPeer::CreatePeerForElement(owner.as<Markdown::MarkdownView>());
        return ProviderFromPeer(peer);
    }

    void ReadingObjectProvider::Expand()
    {
        auto lifetime = get_strong();
        auto state = _state;
        if (state->kind != ReadingChildKind::Disclosure)
            throw hresult_error(UIA_E_NOTSUPPORTED);
        Owner()->SetReadingDisclosure(*state->snapshot, state->index, true);
    }

    void ReadingObjectProvider::Collapse()
    {
        auto lifetime = get_strong();
        auto state = _state;
        if (state->kind != ReadingChildKind::Disclosure)
            throw hresult_error(UIA_E_NOTSUPPORTED);
        Owner()->SetReadingDisclosure(*state->snapshot, state->index, false);
    }

    Windows::UI::Xaml::Automation::ExpandCollapseState ReadingObjectProvider::ExpandCollapseState()
    {
        auto lifetime = get_strong();
        auto state = _state;
        if (state->kind != ReadingChildKind::Disclosure)
            throw hresult_error(UIA_E_NOTSUPPORTED);
        return Owner()->ReadingDisclosureExpanded(*state->snapshot, state->index)
            ? Windows::UI::Xaml::Automation::ExpandCollapseState::Expanded
            : Windows::UI::Xaml::Automation::ExpandCollapseState::Collapsed;
    }
    ReadingObjectProvider::Range ReadingObjectProvider::TextRange()
    {
        auto lifetime = get_strong();
        auto state = _state;
        Owner();
        auto container = TextContainer();
        Owner();
        return make<TextRangeProvider>(state->owner, container, state->snapshot, state->snapshot->token,
                                       static_cast<int32_t>(state->span.start), static_cast<int32_t>(state->span.end));
    }
    HRESULT __stdcall ReadingObjectProvider::GetObjectIdentity(uint64_t *token, uint32_t *start, uint32_t *end) noexcept
    {
        if (!token || !start || !end)
            return E_POINTER;
        *token = _state->snapshot->token;
        *start = _state->span.start;
        *end = _state->span.end;
        return S_OK;
    }
}
