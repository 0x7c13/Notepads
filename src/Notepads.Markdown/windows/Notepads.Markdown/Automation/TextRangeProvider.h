// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Control/MarkdownView.h"

namespace Notepads::Markdown::Native
{
    // Private identity interface prevents unsafe get_self casts of a foreign
    // provider. The token is a monotonic native ID, never an internal pointer.
    struct __declspec(uuid("332C45A3-782B-4B30-9BEA-576C979718D1")) __declspec(novtable) IRangeIdentity : IUnknown
    {
        virtual HRESULT __stdcall GetRangeIdentity(uint64_t *token, int32_t *start, int32_t *end) noexcept = 0;
    };
}

namespace winrt::Notepads::Markdown::implementation
{
    struct TextRangeProvider
        : implements<TextRangeProvider, Windows::UI::Xaml::Automation::Provider::ITextRangeProvider,
                     ::Notepads::Markdown::Native::IRangeIdentity>
    {
        using Range = Windows::UI::Xaml::Automation::Provider::ITextRangeProvider;
        using Provider = Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple;
        using Endpoint = Windows::UI::Xaml::Automation::Text::TextPatternRangeEndpoint;
        using Unit = Windows::UI::Xaml::Automation::Text::TextUnit;
        TextRangeProvider(weak_ref<MarkdownView> owner, Provider provider,
                          std::shared_ptr<TextSnapshot const> snapshot);
        TextRangeProvider(weak_ref<MarkdownView> owner, Provider provider, std::shared_ptr<TextSnapshot const> snapshot,
                          uint64_t token, int32_t start, int32_t end, bool documentOrigin = false);
        Range Clone();
        bool Compare(Range const &other);
        int32_t CompareEndpoints(Endpoint endpoint, Range const &other, Endpoint otherEndpoint);
        void ExpandToEnclosingUnit(Unit unit);
        Range FindAttribute(int32_t, Windows::Foundation::IInspectable const &, bool);
        Range FindText(hstring const &, bool, bool);
        Windows::Foundation::IInspectable GetAttributeValue(int32_t);
        void GetBoundingRectangles(com_array<double> &value);
        Provider GetEnclosingElement();
        hstring GetText(int32_t maximum);
        int32_t Move(Unit unit, int32_t count);
        int32_t MoveEndpointByUnit(Endpoint endpoint, Unit unit, int32_t count);
        void MoveEndpointByRange(Endpoint endpoint, Range const &other, Endpoint otherEndpoint);
        void Select();
        void AddToSelection();
        void RemoveFromSelection();
        void ScrollIntoView(bool);
        com_array<Provider> GetChildren();
        HRESULT __stdcall GetRangeIdentity(uint64_t *token, int32_t *start, int32_t *end) noexcept override;

      private:
        using Child = std::pair<::Notepads::Markdown::Native::ReadingChildKind, size_t>;
        std::optional<Child> EnclosingTableChild(MarkdownView const &owner,
            ::Notepads::Markdown::Native::ReadingSpan span) const;
        com_ptr<MarkdownView> LiveOwner() const;
        struct State
        {
            weak_ref<MarkdownView> owner;
            Provider provider;
            std::shared_ptr<TextSnapshot const> snapshot;
            uint64_t token;
            bool documentOrigin{};
            struct Endpoints
            {
                int32_t start{};
                int32_t end{};
            };
            State(weak_ref<MarkdownView> control, Provider element, std::shared_ptr<TextSnapshot const> text,
                  uint64_t identity, int32_t start, int32_t end, bool document)
                : owner(std::move(control)), provider(std::move(element)), snapshot(std::move(text)), token(identity),
                  documentOrigin(document), _endpoints{start, end}
            {
            }
            Endpoints ReadEndpoints() const noexcept
            {
                winrt::slim_shared_lock_guard guard{_endpointsLock};
                return _endpoints;
            }
            void WriteEndpoints(Endpoints value) noexcept
            {
                winrt::slim_lock_guard guard{_endpointsLock};
                _endpoints = value;
            }

          private:
            // Immutable snapshot owners can be used across apartments. Only the
            // mutable endpoint pair requires synchronization; never hold this
            // lock while allocating, calling XAML, or consulting another range.
            mutable winrt::slim_mutex _endpointsLock;
            Endpoints _endpoints;
        };
        std::shared_ptr<State> _state;
    };
}
