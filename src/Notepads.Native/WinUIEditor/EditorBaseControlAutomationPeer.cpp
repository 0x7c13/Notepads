// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "EditorBaseControlAutomationPeer.h"
#include "EditorBaseControlAutomationPeer.g.cpp"
#include "TextRangeProvider.h"
#include "Helpers.h"
#include "EditorBaseControl.h"
#include <UIAutomationCore.h>
#include <UIAutomationCoreApi.h>

using namespace ::WinUIEditor;
using namespace winrt;
using namespace Windows::Foundation;
using namespace DUX::Automation;
using namespace DUX::Automation::Peers;
using namespace DUX::Automation::Provider;
using namespace DUX::Automation::Text;

namespace winrt::WinUIEditor::implementation
{
	using ITextRangeProvider = DUX::Automation::Provider::ITextRangeProvider;
	using IRawElementProviderSimple = DUX::Automation::Provider::IRawElementProviderSimple;
	using SupportedTextSelection = DUX::Automation::SupportedTextSelection;

	EditorBaseControlAutomationPeer::EditorBaseControlAutomationPeer(WinUIEditor::EditorBaseControl const &owner) : base_type(owner)
	{
		_textRevision = get_self<EditorBaseControl>(owner)->DocumentRevision();
		_updateUIRevoker = owner.Editor().UpdateUI(auto_revoke, { this, &EditorBaseControlAutomationPeer::Editor_UpdateUI });
	}

	void EditorBaseControlAutomationPeer::Editor_UpdateUI(Editor const &sender, UpdateUIEventArgs const &args)
	{
		// https://github.com/microsoft/terminal/blob/main/src/cascadia/TerminalControl/TermControlAutomationPeer.cpp#L133
		if (static_cast<int>(static_cast<Update>(args.Updated()) & Update::Content))
		{
			// Scintilla's Content flag also includes styling and marker changes.
			// Notify assistive technology only when the document text changed.
			const auto revision = get_self<EditorBaseControl>(Owner().as<WinUIEditor::EditorBaseControl>())->DocumentRevision();
			if (revision != _textRevision)
			{
				_textRevision = revision;
				this->RaiseAutomationEvent(AutomationEvents::TextPatternOnTextChanged);
			}
		}
		if (static_cast<int>(static_cast<Update>(args.Updated()) & Update::Selection))
		{
			this->RaiseAutomationEvent(AutomationEvents::TextPatternOnTextSelectionChanged);
		}
	}

	hstring EditorBaseControlAutomationPeer::GetLocalizedControlTypeCore()
	{
		// Todo: Localize
		// Todo: Make sure this is a good name
		return L"code editor";
	}

	IInspectable EditorBaseControlAutomationPeer::GetPatternCore(PatternInterface const &patternInterface)
	{
		// Todo: Should we forward scroll elements? https://learn.microsoft.com/en-us/windows/apps/design/accessibility/custom-automation-peers#forwarding-patterns-from-sub-elements
		switch (patternInterface)
		{
		case PatternInterface::TextEdit:
		case PatternInterface::Text:
		case PatternInterface::Text2:
		case PatternInterface::Value:
			return *this;
		default:
			return __super::GetPatternCore(patternInterface);
		}
	}

	hstring EditorBaseControlAutomationPeer::GetClassNameCore()
	{
		return hstring{ name_of<WinUIEditor::EditorBaseControl>() };
	}

	AutomationControlType EditorBaseControlAutomationPeer::GetAutomationControlTypeCore()
	{
		return AutomationControlType::Edit;
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::DocumentRange()
	{
		// Todo: Look at how WinUI 2 handles GetImpl in automation peers
		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };
		return make<TextRangeProvider>(ProviderFromPeer(*this), editor, 0, editor.Length(), GetBoundingRectangle());
	}

	SupportedTextSelection EditorBaseControlAutomationPeer::SupportedTextSelection()
	{
		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };
		return editor.MultipleSelection() ? SupportedTextSelection::Multiple : SupportedTextSelection::Single;
	}

	com_array<ITextRangeProvider> EditorBaseControlAutomationPeer::GetSelection()
	{
		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };
		const auto n{ static_cast<uint32_t>(editor.Selections()) };
		com_array<ITextRangeProvider> arr(n, nullptr);
		const auto topLeft{ GetBoundingRectangle() };
		for (uint32_t i{ 0 }; i < n; i++)
		{
			const auto start{ editor.GetSelectionNStart(i) };
			const auto end{ editor.GetSelectionNEnd(i) };
			arr[i] = make<TextRangeProvider>(ProviderFromPeer(*this), editor, start, end, topLeft);
		}
		return arr; // Todo: Trying to retrieve the value of a selection n >= 1 crashes Accessibility Insights
	}

	com_array<ITextRangeProvider> EditorBaseControlAutomationPeer::GetVisibleRanges()
	{
		// Visual Studio implements this line by line
		// Todo: Consider folding and line visible messages
		// Todo: Not sure if this is the best method for top line
		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };

		const auto firstDisplayLine{ editor.FirstVisibleLine() };
		const auto maxLine{ editor.LineCount() - 1 };
		// Scintilla reports viewport rows in display lines, including wrapping.
		// Map them back before using document-line APIs or allocating ranges.
		const auto firstVisibleLine{ std::clamp(editor.DocLineFromVisible(firstDisplayLine), int64_t{ 0 }, maxLine) };
		const auto lastVisibleLine{ std::clamp(editor.DocLineFromVisible(firstDisplayLine + editor.LinesOnScreen()), firstVisibleLine, maxLine) };

		const auto count{ lastVisibleLine - firstVisibleLine + 1 };

		std::vector<ITextRangeProvider> ranges;
		ranges.reserve(static_cast<size_t>(count));
		const auto topLeft{ GetBoundingRectangle() };
		for (int64_t i{ 0 }; i < count; i++)
		{
			const auto line{ firstVisibleLine + i };
			const auto displayRow = editor.VisibleFromDocLine(line);
			if (displayRow > firstDisplayLine + editor.LinesOnScreen() || displayRow + editor.WrapCount(line) <= firstDisplayLine) continue;
			const auto start{ editor.PositionFromLine(line) };
			const auto end{ editor.GetLineEndPosition(line) };
			ranges.push_back(make<TextRangeProvider>(ProviderFromPeer(*this), editor, start, end, topLeft));
		}
		return com_array<ITextRangeProvider>(ranges.begin(), ranges.end());
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::RangeFromChild(IRawElementProviderSimple const &childElement)
	{
		// Todo: Does this need to be implemented?
		return nullptr;
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::RangeFromPoint(Point const &screenLocation)
	{
		const auto rect{ GetBoundingRectangle() };
		const Point point{
			screenLocation.X - rect.X,
			screenLocation.Y - rect.Y };

		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };
		const auto pos{ editor.PositionFromPoint(point.X, point.Y) };
		const auto line{ editor.LineFromPosition(pos) };
		const auto start{ editor.PositionFromLine(line) };
		const auto end{ editor.GetLineEndPosition(line) };

		return make<TextRangeProvider>(ProviderFromPeer(*this), editor, start, end, rect);
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::GetActiveComposition()
	{
		// Todo: implement
		throw hresult_not_implemented{};
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::GetConversionTarget()
	{
		// Todo: implement
		throw hresult_not_implemented{};
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::RangeFromAnnotation(IRawElementProviderSimple const &annotationElement)
	{
		// Todo: implement
		throw hresult_not_implemented{};
	}

	ITextRangeProvider EditorBaseControlAutomationPeer::GetCaretRange(bool &isActive)
	{
		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };

		isActive = editor.Focus();

		const auto pos{ editor.GetSelectionNCaret(editor.MainSelection()) };

		return make<TextRangeProvider>(ProviderFromPeer(*this), editor, pos, pos, GetBoundingRectangle());
	}

	bool EditorBaseControlAutomationPeer::IsReadOnly()
	{
		const auto owner{ Owner().as<WinUIEditor::EditorBaseControl>() };
		const auto editor{ owner.Editor() };

		return !owner.IsEnabled() || editor.ReadOnly();
	}

	hstring EditorBaseControlAutomationPeer::Value()
	{
		const auto editor{ Owner().as<WinUIEditor::EditorBaseControl>().Editor() };

		return editor.GetText(editor.Length()); // Todo: this could potentially be gigabytes of data, so provide a maximum safeguard
	}

	void EditorBaseControlAutomationPeer::SetValue(hstring const &value)
	{
		const auto owner{ Owner().as<WinUIEditor::EditorBaseControl>() };
		const auto editor{ owner.Editor() };

		if (!owner.IsEnabled()) winrt::throw_hresult(static_cast<HRESULT>(UIA_E_ELEMENTNOTENABLED));
		if (editor.ReadOnly()) winrt::throw_hresult(static_cast<HRESULT>(UIA_E_INVALIDOPERATION));
		// Use the same counted replacement as native paste. SCI_SETTEXT is
		// NUL-terminated and would truncate an automation-supplied value.
		editor.SelectAll();
		editor.PasteText(value);
	}
}
