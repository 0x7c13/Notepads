// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Automation/TextRangeProvider.h"
#include "Reading/NativeReadingSearch.h"

namespace winrt::Notepads::Markdown::implementation
{
    using namespace Windows::Foundation;
    using ::Notepads::Markdown::Native::ReadingChildKind;
    using ::Notepads::Markdown::Native::ReadingSpan;
    TextRangeProvider::TextRangeProvider(weak_ref<MarkdownView> owner, Provider provider,
                                         std::shared_ptr<TextSnapshot const> snapshot)
        : TextRangeProvider(std::move(owner), std::move(provider), snapshot, snapshot->token,
                            static_cast<int32_t>(snapshot->Bounds().start), static_cast<int32_t>(snapshot->Bounds().end), true)
    {
    }
    TextRangeProvider::TextRangeProvider(weak_ref<MarkdownView> owner, Provider provider,
                                         std::shared_ptr<TextSnapshot const> snapshot, uint64_t token, int32_t start,
                                         int32_t end, bool documentOrigin)
    {
        if (!snapshot || token != snapshot->token || start < 0 || end < start ||
            !snapshot->Bounds().Contains(static_cast<uint32_t>(start), static_cast<uint32_t>(end)))
            throw hresult_invalid_argument();
        if (snapshot->main)
            snapshot->main->ValidateRange(start, end - start);
        if (snapshot->main)
            _state = std::allocate_shared<State>(
                ::Notepads::Markdown::Native::ReadingAllocator<State>{snapshot->main->Memory()},
                std::move(owner), std::move(provider), snapshot, token, start, end, documentOrigin);
        else
            _state = std::make_shared<State>(std::move(owner), std::move(provider), snapshot, token, start, end, documentOrigin);
    }
    com_ptr<MarkdownView> TextRangeProvider::LiveOwner() const
    {
        auto owner = _state->owner.get();
        if (!owner)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        owner->CheckLiveRange(*_state->snapshot);
        return owner;
    }
    TextRangeProvider::Range TextRangeProvider::Clone()
    {
        auto const &s = *_state;
        auto const bounds = s.ReadEndpoints();
        return make<TextRangeProvider>(s.owner, s.provider, s.snapshot, s.token, bounds.start, bounds.end, s.documentOrigin);
    }
    HRESULT __stdcall TextRangeProvider::GetRangeIdentity(uint64_t *token, int32_t *start, int32_t *end) noexcept
    {
        if (!token || !start || !end)
            return E_POINTER;
        auto const bounds = _state->ReadEndpoints();
        *token = _state->token;
        *start = bounds.start;
        *end = bounds.end;
        return S_OK;
    }
    bool TextRangeProvider::Compare(Range const &other)
    {
        auto lifetime = get_strong();
        auto const bounds = _state->ReadEndpoints();
        auto identity = other.try_as<::Notepads::Markdown::Native::IRangeIdentity>();
        uint64_t token{};
        int32_t start{}, end{};
        return identity && SUCCEEDED(identity->GetRangeIdentity(&token, &start, &end)) && token == _state->token &&
               start == bounds.start && end == bounds.end;
    }
    int32_t TextRangeProvider::CompareEndpoints(Endpoint endpoint, Range const &other, Endpoint otherEndpoint)
    {
        auto lifetime = get_strong();
        auto const bounds = _state->ReadEndpoints();
        auto identity = other.try_as<::Notepads::Markdown::Native::IRangeIdentity>();
        uint64_t token{};
        int32_t start{}, end{};
        if (!identity || FAILED(identity->GetRangeIdentity(&token, &start, &end)) || token != _state->token)
            throw hresult_invalid_argument();
        return (endpoint == Endpoint::Start ? bounds.start : bounds.end) -
               (otherEndpoint == Endpoint::Start ? start : end);
    }
    hstring TextRangeProvider::GetText(int32_t maximum)
    {
        if (maximum < -1)
            throw hresult_invalid_argument();
        auto const &s = *_state;
        auto const bounds = s.ReadEndpoints();
        auto length = bounds.end - bounds.start;
        if (maximum >= 0)
            length = std::min(length, maximum);
        if (s.snapshot->main && !s.snapshot->main->IsBoundary(bounds.start + length))
            --length;
        return s.snapshot->Export(bounds.start, length);
    }
    TextRangeProvider::Range TextRangeProvider::FindText(hstring const &needle, bool backward, bool ignoreCase)
    {
        auto const &s = *_state;
        auto const bounds = s.ReadEndpoints();
        if (s.snapshot->main)
        {
            auto match = ::Notepads::Markdown::Native::FindReadingText(
                s.snapshot->View(), {needle.c_str(), needle.size()}, static_cast<uint32_t>(bounds.start),
                static_cast<uint32_t>(bounds.end - bounds.start), backward, ignoreCase);
            if (!match.found)
                return nullptr;
            return make<TextRangeProvider>(s.owner, s.provider, s.snapshot, s.token, static_cast<int32_t>(match.start),
                                           static_cast<int32_t>(match.start + match.length));
        }
        // M0 is a raw UTF16 diagnostic, not a scalar-validated Main projection.
        // Keep its original code-unit matching, including half-surrogate queries.
        if (needle.empty())
            throw hresult_invalid_argument();
        auto text = s.snapshot->View();
        auto count = static_cast<int64_t>(needle.size());
        auto first = static_cast<int64_t>(bounds.start), last = static_cast<int64_t>(bounds.end) - count;
        for (auto position = backward ? last : first; position >= first && position <= last;
             position += backward ? -1 : 1)
        {
            bool equal = ignoreCase ? CompareStringOrdinal(text.data() + position, static_cast<int>(count),
                                                           needle.c_str(), static_cast<int>(count), TRUE) == CSTR_EQUAL
                                    : std::char_traits<wchar_t>::compare(text.data() + position, needle.c_str(),
                                                                         static_cast<size_t>(count)) == 0;
            if (equal)
                return make<TextRangeProvider>(s.owner, s.provider, s.snapshot, s.token, static_cast<int32_t>(position),
                                               static_cast<int32_t>(position + count));
        }
        return nullptr;
    }

    void TextRangeProvider::ExpandToEnclosingUnit(Unit unit)
    {
        auto owner = LiveOwner();
        auto &s = *_state;
        auto bounds = s.ReadEndpoints();
        if (!s.snapshot->main)
        {
            if (unit != Unit::Document)
                throw hresult_not_implemented();
            bounds.start = 0;
            bounds.end = static_cast<int32_t>(s.snapshot->View().size());
            s.WriteEndpoints(bounds);
            return;
        }
        auto span = owner->ReadingNavigation(*s.snapshot).Unit(bounds.start, unit);
        bounds.start = static_cast<int32_t>(span.start);
        bounds.end = static_cast<int32_t>(span.end);
        s.WriteEndpoints(bounds);
    }
    int32_t TextRangeProvider::Move(Unit unit, int32_t count)
    {
        auto owner = LiveOwner();
        auto &s = *_state;
        auto bounds = s.ReadEndpoints();
        if (!s.snapshot->main)
            throw hresult_not_implemented();
        auto const &nav = owner->ReadingNavigation(*s.snapshot);
        if (!count)
            return 0;
        int32_t moved{};
        if (bounds.start == bounds.end)
        {
            auto point = nav.Move(bounds.start, unit, count, moved);
            bounds.start = bounds.end = static_cast<int32_t>(point);
            s.WriteEndpoints(bounds);
            return moved;
        }
        auto initial = nav.Unit(bounds.start, unit);
        auto point = nav.Move(initial.start, unit, count, moved);
        auto span = nav.Unit(point, unit);
        if (point > span.start && moved > 0)
            --moved;
        bounds.start = static_cast<int32_t>(span.start);
        bounds.end = static_cast<int32_t>(span.end);
        s.WriteEndpoints(bounds);
        return moved;
    }
    int32_t TextRangeProvider::MoveEndpointByUnit(Endpoint endpoint, Unit unit, int32_t count)
    {
        auto owner = LiveOwner();
        auto &s = *_state;
        auto bounds = s.ReadEndpoints();
        if (!s.snapshot->main)
            throw hresult_not_implemented();
        int32_t moved{};
        auto point = static_cast<int32_t>(owner->ReadingNavigation(*s.snapshot)
                                              .Move(endpoint == Endpoint::Start ? bounds.start : bounds.end, unit, count, moved));
        if (endpoint == Endpoint::Start)
        {
            bounds.start = point;
            bounds.end = std::max(bounds.end, point);
        }
        else
        {
            bounds.end = point;
            bounds.start = std::min(bounds.start, point);
        }
        s.WriteEndpoints(bounds);
        return moved;
    }
    void TextRangeProvider::MoveEndpointByRange(Endpoint endpoint, Range const &other, Endpoint otherEndpoint)
    {
        auto lifetime = get_strong();
        (void)LiveOwner();
        auto &s = *_state;
        auto identity = other.try_as<::Notepads::Markdown::Native::IRangeIdentity>();
        uint64_t token{};
        int32_t start{}, end{};
        if (!identity || FAILED(identity->GetRangeIdentity(&token, &start, &end)) || token != s.token)
            throw hresult_invalid_argument();
        auto bounds = s.ReadEndpoints();
        auto position = otherEndpoint == Endpoint::Start ? start : end;
        if (position < 0 || !s.snapshot->Bounds().Contains(static_cast<uint32_t>(position), static_cast<uint32_t>(position)))
            throw hresult_invalid_argument();
        if (s.snapshot->main)
            s.snapshot->main->ValidateRange(position, 0);
        if (endpoint == Endpoint::Start)
        {
            bounds.start = position;
            bounds.end = std::max(bounds.end, bounds.start);
        }
        else
        {
            bounds.end = position;
            bounds.start = std::min(bounds.start, bounds.end);
        }
        s.WriteEndpoints(bounds);
    }
    std::optional<TextRangeProvider::Child> TextRangeProvider::EnclosingTableChild(
        MarkdownView const &owner, ReadingSpan span) const
    {
        auto const &s = *_state;
        if (!s.snapshot->descriptors)
            return {};
        auto bounds = s.snapshot->Bounds();
        if (s.documentOrigin && bounds.start == span.start && bounds.end == span.end)
            return {};
        auto const &desc = *s.snapshot->descriptors;
        auto table = desc.TableContaining(span);
        if (!table || !owner.ReadingChildVisible(*s.snapshot, table->text))
            return {};
        if (auto link = desc.LinkAt(span.start);
            link && desc.links[*link].text.Contains(span.start, span.end))
            return Child{ReadingChildKind::Link, *link};
        auto object = std::lower_bound(desc.objects.begin(), desc.objects.end(), span.start,
            [](auto const &row, uint32_t position) { return row.text.start < position; });
        if (object != desc.objects.end() && object->text.start == span.start &&
            object->text.Contains(span.start, span.end))
            return Child{ReadingChildKind::Object, static_cast<size_t>(object - desc.objects.begin())};
        auto kind = span.start < table->caption.end && table->caption.Contains(span.start, span.end)
            ? ReadingChildKind::Caption : ReadingChildKind::Table;
        return Child{kind, static_cast<size_t>(table - desc.tables.data())};
    }

    TextRangeProvider::Provider TextRangeProvider::GetEnclosingElement()
    {
        auto lifetime = get_strong();
        auto const &s = *_state;
        auto owner = LiveOwner();
        auto bounds = s.ReadEndpoints();
        if (auto child = EnclosingTableChild(*owner, {static_cast<uint32_t>(bounds.start), static_cast<uint32_t>(bounds.end)}))
            return owner->ReadingObjectProvider(s.snapshot, child->first, child->second);
        return s.provider;
    }
    void TextRangeProvider::GetBoundingRectangles(com_array<double> &value)
    {
        auto owner = LiveOwner();
        auto const &s = *_state;
        auto const bounds = s.ReadEndpoints();
        if (!s.snapshot->main)
            throw hresult_not_implemented();
        if (bounds.start == bounds.end)
        {
            value = {};
            return;
        }
        auto rectangles = owner->ReadingRectangles(*s.snapshot, bounds.start, bounds.end);
        ::Notepads::Markdown::Native::ReadingVector<double> output{
            ::Notepads::Markdown::Native::ReadingAllocator<double>{s.snapshot->main->Memory()}};
        output.reserve(rectangles.size() * 4);
        for (auto rectangle : rectangles)
        {
            auto screen = owner->ReadingScreenRectangle(rectangle);
            output.push_back(screen.X);
            output.push_back(screen.Y);
            output.push_back(screen.Width);
            output.push_back(screen.Height);
        }
        value = com_array<double>(output.begin(), output.end());
    }
    com_array<TextRangeProvider::Provider> TextRangeProvider::GetChildren()
    {
        auto lifetime = get_strong();
        auto const &s = *_state;
        auto owner = LiveOwner();
        auto const bounds = s.ReadEndpoints();
        if (!s.snapshot->main || !s.snapshot->descriptors)
            return {};
        ::Notepads::Markdown::Native::ReadingVector<Provider> children{
            ::Notepads::Markdown::Native::ReadingAllocator<Provider>{s.snapshot->main->Memory()}};
        auto const &desc = *s.snapshot->descriptors;
        ReadingSpan range{static_cast<uint32_t>(bounds.start), static_cast<uint32_t>(bounds.end)};
        auto append = [&](ReadingChildKind kind, size_t index)
        {
            auto span = desc.Child(kind, index);
            bool group = kind == ReadingChildKind::Table || kind == ReadingChildKind::Caption;
            bool matches = group ? span.start < range.end && range.start < span.end
                                 : range.Contains(span.start, span.end);
            if (matches && owner->ReadingChildVisible(*s.snapshot, span))
                children.push_back(owner->ReadingObjectProvider(s.snapshot, kind, index));
            return true;
        };
        if (auto enclosing = EnclosingTableChild(*owner, range))
        {
            if (enclosing->first == ReadingChildKind::Table || enclosing->first == ReadingChildKind::Caption)
                desc.VisitTableChildren(enclosing->second, enclosing->first == ReadingChildKind::Caption, append);
        }
        else
        {
            desc.VisitRootChildren(append);
        }
        return com_array<Provider>(children.begin(), children.end());
    }
    void TextRangeProvider::Select()
    {
        auto owner = LiveOwner();
        auto const bounds = _state->ReadEndpoints();
        owner->SelectReadingRange(*_state->snapshot, bounds.start, bounds.end);
    }
    void TextRangeProvider::AddToSelection()
    {
        auto owner = LiveOwner();
        auto const bounds = _state->ReadEndpoints();
        auto [start, end] = owner->SelectionBounds();
        if (start != end &&
            (start != static_cast<uint32_t>(bounds.start) || end != static_cast<uint32_t>(bounds.end)))
            throw hresult_error(UIA_E_INVALIDOPERATION);
        owner->SelectReadingRange(*_state->snapshot, bounds.start, bounds.end);
    }
    void TextRangeProvider::RemoveFromSelection()
    {
        auto owner = LiveOwner();
        auto const bounds = _state->ReadEndpoints();
        owner->RemoveReadingSelection(*_state->snapshot, static_cast<uint32_t>(bounds.start),
                                      static_cast<uint32_t>(bounds.end));
    }
    void TextRangeProvider::ScrollIntoView(bool alignTop)
    {
        auto owner = LiveOwner();
        auto const bounds = _state->ReadEndpoints();
        owner->RevealReadingRange(*_state->snapshot, bounds.start, bounds.end, alignTop, true, true);
    }
    TextRangeProvider::Range TextRangeProvider::FindAttribute(int32_t attribute, IInspectable const &value, bool)
    {
        auto lifetime = get_strong();
        (void)LiveOwner();
        if (attribute == static_cast<int32_t>(Windows::UI::Xaml::AutomationTextAttributesEnum::IsReadOnlyAttribute))
        {
            auto property = value.try_as<IPropertyValue>();
            if (!property || property.Type() != PropertyType::Boolean)
                throw hresult_invalid_argument();
            return property.GetBoolean() ? Clone() : nullptr;
        }
        return nullptr;
    }
    IInspectable TextRangeProvider::GetAttributeValue(int32_t attribute)
    {
        auto owner = LiveOwner();
        if (attribute == static_cast<int32_t>(Windows::UI::Xaml::AutomationTextAttributesEnum::IsReadOnlyAttribute))
            return box_value(true);
        com_ptr<::IUnknown> reserved;
        if (attribute == static_cast<int32_t>(Windows::UI::Xaml::AutomationTextAttributesEnum::IsHiddenAttribute))
        {
            auto const bounds = _state->ReadEndpoints();
            auto value = owner->ReadingRangeHidden(*_state->snapshot,
                {static_cast<uint32_t>(bounds.start), static_cast<uint32_t>(bounds.end)});
            if (value)
                return box_value(*value);
            check_hresult(UiaGetReservedMixedAttributeValue(reserved.put()));
        }
        else
            check_hresult(UiaGetReservedNotSupportedValue(reserved.put()));
        return reserved.as<IInspectable>();
    }
}
