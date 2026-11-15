// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "MarkdownViewAutomationPeer.g.h"
#include "Control/MarkdownView.h"

namespace winrt::Notepads::Markdown::implementation
{
    struct MarkdownViewAutomationPeer : MarkdownViewAutomationPeerT<MarkdownViewAutomationPeer>
    {
        using Range = Windows::UI::Xaml::Automation::Provider::ITextRangeProvider;
        MarkdownViewAutomationPeer(Markdown::MarkdownView const &owner);
        hstring GetClassNameCore();
        Windows::UI::Xaml::Automation::Peers::AutomationControlType GetAutomationControlTypeCore();
        Windows::Foundation::IInspectable GetPatternCore(
            Windows::UI::Xaml::Automation::Peers::PatternInterface pattern);
        Windows::Foundation::Collections::IVector<Windows::UI::Xaml::Automation::Peers::AutomationPeer>
        GetChildrenCore();
        Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple Raw();
        Range DocumentRange();
        Windows::UI::Xaml::Automation::SupportedTextSelection SupportedTextSelection();
        com_array<Range> GetSelection();
        com_array<Range> GetVisibleRanges();
        Range RangeFromChild(Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple const &);
        Range RangeFromPoint(Windows::Foundation::Point const &);

      private:
        weak_ref<MarkdownView> _owner;
    };
}

namespace winrt::Notepads::Markdown::factory_implementation
{
    struct MarkdownViewAutomationPeer
        : MarkdownViewAutomationPeerT<MarkdownViewAutomationPeer, implementation::MarkdownViewAutomationPeer>
    {
    };
}
