// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "ReadingObjectProvider.g.h"
#include "Control/MarkdownView.h"

namespace Notepads::Markdown::Native
{
    struct __declspec(uuid("41F4FE7E-F742-4B58-B9ED-710C510644A2")) __declspec(novtable) IReadingObjectIdentity
        : IUnknown
    {
        virtual HRESULT __stdcall GetObjectIdentity(uint64_t *token, uint32_t *start, uint32_t *end) noexcept = 0;
    };
}
namespace winrt::Notepads::Markdown::implementation
{
    struct ReadingObjectProvider
        : ReadingObjectProviderT<ReadingObjectProvider, ::Notepads::Markdown::Native::IReadingObjectIdentity>
    {
        using Provider = Windows::UI::Xaml::Automation::Provider::IRawElementProviderSimple;
        using Range = Windows::UI::Xaml::Automation::Provider::ITextRangeProvider;
        ReadingObjectProvider(weak_ref<MarkdownView> owner, std::shared_ptr<TextSnapshot const> snapshot,
                              ::Notepads::Markdown::Native::ReadingChildKind kind,
                              size_t index);
        Provider Raw();
        hstring GetClassNameCore();
        hstring GetNameCore();
        hstring GetHelpTextCore();
        void UpdateMathHelpText(hstring value);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        uint32_t MathHelpTextChangesForDiagnostics() const noexcept { return _mathHelpTextChanges; }
#endif
        Windows::UI::Xaml::Automation::Peers::AutomationPeer GetLabeledByCore();
        Windows::UI::Xaml::Automation::Peers::AutomationControlType GetAutomationControlTypeCore();
        Windows::Foundation::IInspectable GetPatternCore(
            Windows::UI::Xaml::Automation::Peers::PatternInterface pattern);
        Windows::Foundation::Rect GetBoundingRectangleCore();
        Windows::Foundation::Point GetClickablePointCore();
        bool IsOffscreenCore();
        bool IsEnabledCore();
        bool IsKeyboardFocusableCore();
        bool HasKeyboardFocusCore();
        bool IsControlElementCore();
        bool IsContentElementCore();
        void SetFocusCore();
        Windows::Foundation::Collections::IVector<Windows::UI::Xaml::Automation::Peers::AutomationPeer>
        GetChildrenCore();
        void Invoke();
        bool HorizontallyScrollable();
        bool VerticallyScrollable();
        double HorizontalScrollPercent();
        double VerticalScrollPercent();
        double HorizontalViewSize();
        double VerticalViewSize();
        void SetScrollPercent(double horizontal, double vertical);
        void Scroll(Windows::UI::Xaml::Automation::ScrollAmount const &horizontal,
                    Windows::UI::Xaml::Automation::ScrollAmount const &vertical);
        bool CanMove() noexcept { return false; }
        bool CanResize() noexcept { return false; }
        bool CanRotate() noexcept { return false; }
        void Move(double, double) { throw hresult_error(UIA_E_NOTSUPPORTED); }
        void Resize(double, double) { throw hresult_error(UIA_E_NOTSUPPORTED); }
        void Rotate(double) { throw hresult_error(UIA_E_NOTSUPPORTED); }
        bool CanZoom();
        double ZoomLevel();
        double MinZoom();
        double MaxZoom();
        void Zoom(double zoom);
        void ZoomByUnit(Windows::UI::Xaml::Automation::ZoomUnit const &unit);
        void Toggle();
        Windows::UI::Xaml::Automation::ToggleState ToggleState();
        void Expand();
        void Collapse();
        Windows::UI::Xaml::Automation::ExpandCollapseState ExpandCollapseState();
        Provider TextContainer();
        Range TextRange();
        HRESULT __stdcall GetObjectIdentity(uint64_t *token, uint32_t *start, uint32_t *end) noexcept override;

      private:
        struct State
        {
            weak_ref<MarkdownView> owner;
            std::shared_ptr<TextSnapshot const> snapshot;
            ::Notepads::Markdown::Native::ReadingChildKind kind;
            size_t index;
            ::Notepads::Markdown::Native::ReadingSpan span;
        };
        std::shared_ptr<State> _state;
        hstring _mathHelpText;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        uint32_t _mathHelpTextChanges{};
#endif
        com_ptr<MarkdownView> Owner() const;
        bool IsDiagram() const noexcept;
        MarkdownView::ReadingDiagramInspection RequireDiagramInspection(MarkdownView const &owner) const;
    };
}
