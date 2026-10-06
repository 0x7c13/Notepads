#pragma once

#include <winrt/WinUIEditor.h>

namespace winrt::WinUIEditor::implementation
{
	struct TextRangeProvider : implements<TextRangeProvider, DUX::Automation::Provider::ITextRangeProvider>
	{
		TextRangeProvider(DUX::Automation::Provider::IRawElementProviderSimple const &peer, WinUIEditor::Editor const &editor, int64_t rangeStart, int64_t rangeEnd, Windows::Foundation::Rect const &bounds);

		DUX::Automation::Provider::ITextRangeProvider Clone();
		bool Compare(DUX::Automation::Provider::ITextRangeProvider const &textRangeProvider);
		int32_t CompareEndpoints(DUX::Automation::Text::TextPatternRangeEndpoint const &endpoint, DUX::Automation::Provider::ITextRangeProvider const &textRangeProvider, DUX::Automation::Text::TextPatternRangeEndpoint const &targetEndpoint);
		void ExpandToEnclosingUnit(DUX::Automation::Text::TextUnit const &unit);
		DUX::Automation::Provider::ITextRangeProvider FindAttribute(int32_t attributeId, Windows::Foundation::IInspectable const &value, bool backward);
		DUX::Automation::Provider::ITextRangeProvider FindText(hstring const &text, bool backward, bool ignoreCase);
		Windows::Foundation::IInspectable GetAttributeValue(int32_t attributeId);
		void GetBoundingRectangles(com_array<double> &returnValue);
		DUX::Automation::Provider::IRawElementProviderSimple GetEnclosingElement();
		hstring GetText(int32_t maxLength);
		int32_t Move(DUX::Automation::Text::TextUnit const &unit, int32_t count);
		int32_t MoveEndpointByUnit(DUX::Automation::Text::TextPatternRangeEndpoint const &endpoint, DUX::Automation::Text::TextUnit const &unit, int32_t count);
		void MoveEndpointByRange(DUX::Automation::Text::TextPatternRangeEndpoint const &endpoint, DUX::Automation::Provider::ITextRangeProvider const &textRangeProvider, DUX::Automation::Text::TextPatternRangeEndpoint const &targetEndpoint);
		void Select();
		void AddToSelection();
		void RemoveFromSelection();
		void ScrollIntoView(bool alignToTop);
		com_array<DUX::Automation::Provider::IRawElementProviderSimple> GetChildren();


	private:
		DUX::Automation::Provider::IRawElementProviderSimple _peer{ nullptr };
		weak_ref<WinUIEditor::Editor> _editor{ nullptr };
		int64_t _rangeStart;
		int64_t _rangeEnd;
		Windows::Foundation::Rect _bounds;
	};
}
