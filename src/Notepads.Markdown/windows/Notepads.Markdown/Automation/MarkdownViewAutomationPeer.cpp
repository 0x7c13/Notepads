// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Automation/MarkdownViewAutomationPeer.h"
#include "MarkdownViewAutomationPeer.g.cpp"
#include "Automation/TextRangeProvider.h"
#include "Automation/ReadingObjectProvider.h"

namespace winrt::Notepads::Markdown::implementation
{
    using namespace Windows::Foundation;
    using namespace Windows::UI::Xaml::Automation;
    using namespace Windows::UI::Xaml::Automation::Peers;

    namespace
    {
        struct ReadingScrollProvider : implements<ReadingScrollProvider, Provider::IScrollProvider>
        {
            ReadingScrollProvider(weak_ref<MarkdownView> owner, Windows::UI::Xaml::Controls::ScrollViewer const &scroll)
                : _owner(std::move(owner)), _scroll(make_weak(scroll))
            {
            }
            Provider::IScrollProvider Target() const
            {
                auto owner = _owner.get();
                auto scroll = _scroll.get();
                if (!owner || !scroll || owner->ReadingScrollViewer() != scroll)
                    throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
                auto peer = FrameworkElementAutomationPeer::FromElement(scroll);
                if (!peer)
                    peer = FrameworkElementAutomationPeer::CreatePeerForElement(scroll);
                auto documentPeer = FrameworkElementAutomationPeer::FromElement(owner.as<Markdown::MarkdownView>());
                if (documentPeer)
                    peer.EventsSource(documentPeer);
                auto provider = peer.GetPattern(PatternInterface::Scroll).try_as<Provider::IScrollProvider>();
                if (!provider)
                    throw hresult_error(UIA_E_NOTSUPPORTED);
                return provider;
            }
            bool HorizontallyScrollable()
            {
                return Target().HorizontallyScrollable();
            }
            bool VerticallyScrollable()
            {
                return Target().VerticallyScrollable();
            }
            double HorizontalScrollPercent()
            {
                return Target().HorizontalScrollPercent();
            }
            double VerticalScrollPercent()
            {
                return Target().VerticalScrollPercent();
            }
            double HorizontalViewSize()
            {
                return Target().HorizontalViewSize();
            }
            double VerticalViewSize()
            {
                return Target().VerticalViewSize();
            }
            void Scroll(ScrollAmount horizontal, ScrollAmount vertical)
            {
                auto target = Target();
                auto owner = _owner.get();
                auto scroll = _scroll.get();
                if (!owner || !scroll)
                    return;
                const auto y = scroll.VerticalOffset();
                const bool decrement = vertical == ScrollAmount::SmallDecrement || vertical == ScrollAmount::LargeDecrement;
                const bool increment = vertical == ScrollAmount::SmallIncrement || vertical == ScrollAmount::LargeIncrement;
                const bool moves = horizontal == ScrollAmount::NoAmount &&
                    ((decrement && y > 0) || (increment && y < scroll.ScrollableHeight()));
                if (owner->BeginReadingScroll(scroll, moves))
                    target.Scroll(horizontal, vertical);
            }
            void SetScrollPercent(double horizontal, double vertical)
            {
                auto target = Target();
                auto owner = _owner.get();
                auto scroll = _scroll.get();
                if (!owner || !scroll)
                    return;
                const bool moves = horizontal == -1 && vertical >= 0 && vertical <= 100 && target.VerticallyScrollable() &&
                                   vertical != target.VerticalScrollPercent();
                if (owner->BeginReadingScroll(scroll, moves))
                    target.SetScrollPercent(horizontal, vertical);
            }

          private:
            weak_ref<MarkdownView> _owner;
            weak_ref<Windows::UI::Xaml::Controls::ScrollViewer> _scroll;
        };
    }

    MarkdownViewAutomationPeer::MarkdownViewAutomationPeer(Markdown::MarkdownView const &owner)
        : base_type(owner), _owner(get_self<MarkdownView>(owner)->get_weak())
    {
    }

    hstring MarkdownViewAutomationPeer::GetClassNameCore()
    {
        return L"Notepads.Markdown.MarkdownView";
    }
    AutomationControlType MarkdownViewAutomationPeer::GetAutomationControlTypeCore()
    {
        return AutomationControlType::Document;
    }

    IInspectable MarkdownViewAutomationPeer::GetPatternCore(PatternInterface pattern)
    {
        if (pattern == PatternInterface::Text)
            return *this;
        if (pattern == PatternInterface::Scroll)
        {
            auto owner = _owner.get();
            if (!owner)
                throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
            auto scroll = owner->ReadingScrollViewer();
            if (!scroll)
                return nullptr;
            auto provider = make_self<ReadingScrollProvider>(_owner, scroll);
            // The native ScrollViewer peer remains the source of scroll values and events.
            (void)provider->Target();
            return provider.as<IInspectable>();
        }
        return base_type::GetPatternCore(pattern);
    }

    MarkdownViewAutomationPeer::Range MarkdownViewAutomationPeer::DocumentRange()
    {
        auto owner = _owner.get();
        if (!owner)
        {
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        }
        return make<TextRangeProvider>(_owner, ProviderFromPeer(*this), owner->AutomationSnapshot());
    }

    Collections::IVector<AutomationPeer> MarkdownViewAutomationPeer::GetChildrenCore()
    {
        auto owner = _owner.get();
        return owner ? owner->ReadingChildren() : nullptr;
    }
    Windows::UI::Xaml::Automation::SupportedTextSelection MarkdownViewAutomationPeer::SupportedTextSelection()
    {
        auto owner = _owner.get();
        return owner && owner->ReadingAvailable() ? Windows::UI::Xaml::Automation::SupportedTextSelection::Single
                                                  : Windows::UI::Xaml::Automation::SupportedTextSelection::None;
    }
    Provider::IRawElementProviderSimple MarkdownViewAutomationPeer::Raw()
    {
        return ProviderFromPeer(*this);
    }
    com_array<MarkdownViewAutomationPeer::Range> MarkdownViewAutomationPeer::GetSelection()
    {
        auto owner = _owner.get();
        if (!owner || !owner->ReadingAvailable())
            return {};
        auto snapshot = owner->AutomationSnapshot();
        auto [start, end] = owner->SelectionBounds();
        return {make<TextRangeProvider>(_owner, ProviderFromPeer(*this), snapshot, snapshot->token,
                                        static_cast<int32_t>(start), static_cast<int32_t>(end))};
    }
    com_array<MarkdownViewAutomationPeer::Range> MarkdownViewAutomationPeer::GetVisibleRanges()
    {
        auto owner = _owner.get();
        if (!owner)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto snapshot = owner->AutomationSnapshot();
        if (!snapshot->main)
            throw hresult_not_implemented();
        auto spans = owner->VisibleReadingRanges(*snapshot);
        ::Notepads::Markdown::Native::ReadingVector<Range> ranges{
            ::Notepads::Markdown::Native::ReadingAllocator<Range>{snapshot->main->Memory()}};
        ranges.reserve(spans.size());
        for (auto span : spans)
            ranges.push_back(make<TextRangeProvider>(_owner, ProviderFromPeer(*this), snapshot, snapshot->token,
                                                     static_cast<int32_t>(span.start), static_cast<int32_t>(span.end)));
        return com_array<Range>(ranges.begin(), ranges.end());
    }
    MarkdownViewAutomationPeer::Range MarkdownViewAutomationPeer::RangeFromChild(
        Provider::IRawElementProviderSimple const &child)
    {
        auto owner = _owner.get();
        if (!owner)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto snapshot = owner->AutomationSnapshot();
        owner->CheckLiveRange(*snapshot);
        auto peer = PeerFromProvider(child);
        auto identity = peer ? peer.try_as<::Notepads::Markdown::Native::IReadingObjectIdentity>() : nullptr;
        uint64_t token{};
        uint32_t start{}, end{};
        if (identity)
        {
            if (FAILED(identity->GetObjectIdentity(&token, &start, &end)) || token != snapshot->token)
                throw hresult_invalid_argument();
        }
        else
        {
            auto frameworkPeer = peer.try_as<FrameworkElementAutomationPeer>();
            auto view = frameworkPeer ? frameworkPeer.Owner().try_as<Markdown::MarkdownView>() : nullptr;
            auto span = view ? owner->ReadingNoteChildSpan(*snapshot, view) : std::nullopt;
            if (!span)
                throw hresult_invalid_argument();
            token = snapshot->token;
            start = span->start;
            end = span->end;
        }
        if (!snapshot->Bounds().Contains(start, end))
            throw hresult_invalid_argument();
        if (!owner->ReadingChildVisible(*snapshot, {start, end}))
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        return make<TextRangeProvider>(_owner, ProviderFromPeer(*this), snapshot, token, static_cast<int32_t>(start),
                                       static_cast<int32_t>(end));
    }
    MarkdownViewAutomationPeer::Range MarkdownViewAutomationPeer::RangeFromPoint(Point const &point)
    {
        auto owner = _owner.get();
        if (!owner)
            throw hresult_error(UIA_E_ELEMENTNOTAVAILABLE);
        auto snapshot = owner->AutomationSnapshot();
        auto position = owner->ReadingPositionFromScreen(*snapshot, point);
        return make<TextRangeProvider>(_owner, ProviderFromPeer(*this), snapshot, snapshot->token,
                                       static_cast<int32_t>(position), static_cast<int32_t>(position));
    }
}
