#pragma once

#include "EditorBaseControlAutomationPeer.g.h"

namespace winrt::WinUIEditor::implementation
{
    struct EditorBaseControlAutomationPeer : EditorBaseControlAutomationPeerT<EditorBaseControlAutomationPeer>
    {
        EditorBaseControlAutomationPeer(WinUIEditor::EditorBaseControl const &owner);

        hstring GetLocalizedControlTypeCore();
        Windows::Foundation::IInspectable GetPatternCore(DUX::Automation::Peers::PatternInterface const &patternInterface);
        hstring GetClassNameCore();
        DUX::Automation::Peers::AutomationControlType GetAutomationControlTypeCore();

        DUX::Automation::Provider::ITextRangeProvider DocumentRange();
        DUX::Automation::SupportedTextSelection SupportedTextSelection();
        com_array<DUX::Automation::Provider::ITextRangeProvider> GetSelection();
        com_array<DUX::Automation::Provider::ITextRangeProvider> GetVisibleRanges();
        DUX::Automation::Provider::ITextRangeProvider RangeFromChild(DUX::Automation::Provider::IRawElementProviderSimple const &childElement);
        DUX::Automation::Provider::ITextRangeProvider RangeFromPoint(Windows::Foundation::Point const &screenLocation);
        DUX::Automation::Provider::ITextRangeProvider GetActiveComposition();
        DUX::Automation::Provider::ITextRangeProvider GetConversionTarget();

        DUX::Automation::Provider::ITextRangeProvider RangeFromAnnotation(DUX::Automation::Provider::IRawElementProviderSimple const &annotationElement);
        DUX::Automation::Provider::ITextRangeProvider GetCaretRange(bool &isActive);

        bool IsReadOnly();
        hstring Value();
        void SetValue(hstring const &value);

    private:
        WinUIEditor::Editor::UpdateUI_revoker _updateUIRevoker{};
        void Editor_UpdateUI(WinUIEditor::Editor const &sender, WinUIEditor::UpdateUIEventArgs const &args);
    };
}

namespace winrt::WinUIEditor::factory_implementation
{
    struct EditorBaseControlAutomationPeer : EditorBaseControlAutomationPeerT<EditorBaseControlAutomationPeer, implementation::EditorBaseControlAutomationPeer>
    {
    };
}
