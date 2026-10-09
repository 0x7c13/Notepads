// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "TextRangeProvider.h"
#include "EditorWrapper.h"

using namespace winrt;
using namespace Windows::Foundation;
using namespace DUX;
using namespace DUX::Automation;
using namespace DUX::Automation::Peers;
using namespace DUX::Automation::Provider;
using namespace DUX::Automation::Text;

constexpr hresult XAML_E_NOT_SUPPORTED{ static_cast<int32_t>(0x80131515) };

namespace winrt::WinUIEditor::implementation
{
	namespace
	{
		int64_t CharacterBoundary(WinUIEditor::Editor const &editor, int64_t position)
		{
			const auto length = editor.Length();
			position = std::clamp(position, int64_t{0}, length);
			// A range can outlive an edit that puts its old byte offset inside
			// a UTF-8 scalar. Navigation must never expose a partial character.
			if (editor.CodePage() == Scintilla::CpUtf8)
				while (position > 0 && position < length &&
					Scintilla::Internal::UTF8IsTrailByte(static_cast<unsigned char>(editor.GetCharAt(position))))
					--position;
			return position;
		}

		int64_t MoveCharacters(WinUIEditor::Editor const &editor, int64_t position, int32_t count)
		{
			const auto length{ editor.Length() };
			position = std::clamp(position, int64_t{ 0 }, length);
			const auto moved{ editor.PositionRelative(position, count) };
			// SCI_POSITIONRELATIVE deliberately returns zero past the document
			// end. Automation movement must instead stop at that end.
			return count > 0 && moved == 0 ? length : moved;
		}

		TextUnit SupportedUnit(TextUnit unit)
		{
			switch (unit)
			{
			case TextUnit::Format: return TextUnit::Word;
			case TextUnit::Page: return TextUnit::Document;
			case TextUnit::Character:
			case TextUnit::Word:
			case TextUnit::Line:
			case TextUnit::Paragraph:
			case TextUnit::Document: return unit;
			default: throw hresult_invalid_argument();
			}
		}

		struct UnitSpan
		{
			int64_t start;
			int64_t end;
		};

		UnitSpan UnitRange(WinUIEditor::Editor const &editor, int64_t position, TextUnit unit)
		{
			const auto length = editor.Length();
			position = std::clamp(position, int64_t{0}, length);
			if (unit == TextUnit::Document)
				return {0, length};
			if (length == 0 || (unit == TextUnit::Character && position == length))
				return {length, length};
			if (unit == TextUnit::Character)
			{
				// Match PositionRelative/CountCharacters: CR and LF are separate
				// scalars, whereas caret PositionAfter groups a CRLF pair.
				position = CharacterBoundary(editor, position);
				return {position, MoveCharacters(editor, position, 1)};
			}
			if (unit == TextUnit::Paragraph)
			{
				const auto line = editor.LineFromPosition(position);
				return {editor.PositionFromLine(line), editor.PositionFromLine(line + 1)};
			}
			const auto range = get_self<implementation::Editor>(editor)->AutomationUnitRange(position, unit == TextUnit::Word);
			return {range.start, range.end};
		}

		int32_t MovePosition(WinUIEditor::Editor const &editor, TextUnit unit, int32_t count, int64_t &position)
		{
			if (unit == TextUnit::Character)
			{
				position = CharacterBoundary(editor, position);
				const auto original = position;
				position = MoveCharacters(editor, position, count);
				const auto moved = editor.CountCharacters(std::min(original, position), std::max(original, position));
				return position < original ? -static_cast<int32_t>(moved) : static_cast<int32_t>(moved);
			}
			int32_t moved = 0;
			const auto length = editor.Length();
			const int direction = count > 0 ? 1 : -1;
			while (moved != count)
			{
				if ((direction > 0 && position == length) || (direction < 0 && position == 0))
					break;
				const auto range = UnitRange(editor, position, unit);
				auto next = direction > 0 ? range.end : range.start;
				if (direction < 0 && next == position)
					next = UnitRange(editor, editor.PositionBefore(position), unit).start;
				if (next == position)
					break;
				position = next;
				moved += direction;
			}
			return moved;
		}
	}

	TextRangeProvider::TextRangeProvider(IRawElementProviderSimple const &peer, WinUIEditor::Editor const &editor, int64_t rangeStart, int64_t rangeEnd, Rect const &bounds)
		: _peer{ peer }, _editor{ editor }, _rangeStart{ rangeStart }, _rangeEnd{ rangeEnd }, _bounds{ bounds }
	{
	}

	ITextRangeProvider TextRangeProvider::Clone()
	{
		return make<TextRangeProvider>(_peer, _editor.get(), _rangeStart, _rangeEnd, _bounds);
	}

	bool TextRangeProvider::Compare(ITextRangeProvider const &textRangeProvider)
	{
		const auto target{ textRangeProvider.as<TextRangeProvider>() };
		return _rangeStart == target->_rangeStart && _rangeEnd == target->_rangeEnd;
	}

	int32_t TextRangeProvider::CompareEndpoints(TextPatternRangeEndpoint const &endpoint, ITextRangeProvider const &textRangeProvider, TextPatternRangeEndpoint const &targetEndpoint)
	{
		const auto target{ textRangeProvider.as<TextRangeProvider>() };
		const auto thisEnd{ endpoint == TextPatternRangeEndpoint::Start ? _rangeStart : _rangeEnd };
		const auto targetEnd{ targetEndpoint == TextPatternRangeEndpoint::Start ? target->_rangeStart : target->_rangeEnd };
		return thisEnd - targetEnd;
	}

	void TextRangeProvider::ExpandToEnclosingUnit(TextUnit const &unit)
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		const auto range = UnitRange(editor, _rangeStart, SupportedUnit(unit));
		_rangeStart = range.start;
		_rangeEnd = range.end;
	}

	ITextRangeProvider TextRangeProvider::FindAttribute(int32_t attributeId, IInspectable const &value, bool backward)
	{
		throw hresult_not_implemented{};
	}

	ITextRangeProvider TextRangeProvider::FindText(hstring const &text, bool backward, bool ignoreCase)
	{
		throw hresult_not_implemented{};
	}

	IInspectable TextRangeProvider::GetAttributeValue(int32_t attributeId)
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		switch (static_cast<AutomationTextAttributesEnum>(attributeId))
		{
		case AutomationTextAttributesEnum::IsReadOnlyAttribute:
			return box_value(editor.ReadOnly());
		case AutomationTextAttributesEnum::CaretPositionAttribute:
		{
			const auto sel{ editor.MainSelection() };
			const auto caret{ editor.GetSelectionNCaret(sel) };
			if (caret < _rangeStart || caret > _rangeEnd)
			{
				// Unknown if caret is outside this text range
				// Todo: Not sure if this is the correct behavior
				return box_value(AutomationCaretPosition::Unknown);
			}
			const auto line{ editor.LineFromPosition(caret) };
			if (caret == editor.PositionFromLine(line))
			{
				return box_value(AutomationCaretPosition::BeginningOfLine);
			}
			else if (caret == editor.GetLineEndPosition(line))
			{
				return box_value(AutomationCaretPosition::EndOfLine);
			}
			else
			{
				return box_value(AutomationCaretPosition::Unknown);
			}
		}
		default:
			// Must throw instead of returning nullptr
			// https://github.com/microsoft/terminal/blob/main/src/cascadia/TerminalControl/XamlUiaTextRange.cpp#L141
			throw_hresult(XAML_E_NOT_SUPPORTED);
		}
	}

	void TextRangeProvider::GetBoundingRectangles(com_array<double> &returnValue)
	{
		// Structure: https://github.com/microsoft/terminal/blob/main/src/types/UiaTextRangeBase.cpp#L836

		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		// Todo: see EditorBaseControlAutomationPeer::GetVisibleRanges for caveats
		// Todo: Does not account for horizontal scrolling
		// Todo: Does not account for ranges that do not start and end at line starts/ends

		const auto firstDisplayLine{ editor.FirstVisibleLine() };
		const auto maxLine{ editor.LineCount() - 1 };
		const auto firstVisibleLine{ std::clamp(editor.DocLineFromVisible(firstDisplayLine), int64_t{0}, maxLine) };
		const auto lastVisibleLine{ std::clamp(editor.DocLineFromVisible(firstDisplayLine + editor.LinesOnScreen()), firstVisibleLine, maxLine) };

		const auto startLine{ editor.LineFromPosition(_rangeStart) };
		const auto endLine{ editor.LineFromPosition(_rangeEnd) };

		const auto top{ std::max(startLine, firstVisibleLine) };
		const auto end{ std::min(endLine, lastVisibleLine) };
		const auto count{ end - top + 1 };

		if (count < 1)
		{
			returnValue = {};
			return;
		}

		auto margins{ editor.MarginLeft() };
		const auto nMargins{ editor.Margins() };
		for (auto i{ 0 }; i < nMargins; i++)
		{
			margins += editor.GetMarginWidthN(i);
		}
		const auto maxTextWidth{ static_cast<int>(_bounds.Width) - editor.MarginRight() };

		std::vector<double> rectangles;
		rectangles.reserve(static_cast<size_t>(count) * 4);
		// Todo: This code pretends that TextHeight gets the correct height including sublines (wrapping). It does not
		for (int64_t i = 0; i < count; i++)
		{
			const auto line{ top + i };
			const auto displayRow = editor.VisibleFromDocLine(line);
			if (displayRow > firstDisplayLine + editor.LinesOnScreen() || displayRow + editor.WrapCount(line) <= firstDisplayLine) continue;
			const auto j = rectangles.size();
			rectangles.resize(j + 4);
			const auto pos{ editor.PositionFromLine(line) };
			const auto xStart{ editor.PointXFromPosition(line == startLine ? _rangeStart : pos) };
			const auto xEnd{ editor.PointXFromPosition((line == endLine ? _rangeEnd : editor.GetLineEndPosition(line))) };
			const auto textStart{ std::max(margins, xStart) };
			rectangles[j + 0] = _bounds.X + textStart; // Left
			rectangles[j + 1] = _bounds.Y + editor.PointYFromPosition(pos); // Top
			rectangles[j + 2] = std::min(maxTextWidth, xEnd) - textStart; // Width
			rectangles[j + 3] = editor.TextHeight(line); // Height

			// Todo: Reconsider how off screen text is handled
			if (xEnd < margins)
			{
				rectangles[j + 0] = _bounds.X + margins;
				rectangles[j + 2] = 0;
			}
			if (xStart > maxTextWidth)
			{
				rectangles[j + 0] = _bounds.X + maxTextWidth;
				rectangles[j + 2] = 0;
			}
		}
		returnValue = com_array<double>(rectangles.begin(), rectangles.end());
	}

	IRawElementProviderSimple TextRangeProvider::GetEnclosingElement()
	{
		return _peer;
	}

	hstring TextRangeProvider::GetText(int32_t maxLength)
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		// Automation ranges can outlive edits. Clamp them to the current document,
		// and count Unicode characters rather than UTF-8 bytes for maxLength.
		const auto documentLength{ editor.Length() };
		const auto start{ std::clamp(_rangeStart, int64_t{ 0 }, documentLength) };
		auto end{ std::clamp(_rangeEnd, start, documentLength) };
		if (maxLength == 0 || end == start)
		{
			return L"";
		}
		if (maxLength > 0)
		{
			end = std::min(end, MoveCharacters(editor, start, maxLength));
		}
		const auto length{ end - start };
		// SCI_GETTEXTRANGEFULL always writes a trailing NUL, even for an empty
		// range. The buffer must have one more byte than the requested text.
		std::string s(static_cast<size_t>(length) + 1, '\0');
		Scintilla::TextRangeFull range
		{
			{ start, end, },
			s.data(),
		};
		editor.GetTextRangeFull(reinterpret_cast<uint64_t>(&range));
		s.resize(static_cast<size_t>(length));
		return to_hstring(s);
	}

	int32_t TextRangeProvider::Move(TextUnit const &unit, int32_t count)
	{
		if (count == 0)
			return 0;
		const auto editor{ _editor.get() };
		if (!editor)
			throw hresult_error{ E_POINTER };
		const auto length{ editor.Length() };
		const auto oldStart{ std::clamp(_rangeStart, int64_t{ 0 }, length) };
		const auto oldEnd{ std::clamp(_rangeEnd, oldStart, length) };
		const auto supported = SupportedUnit(unit);
		const bool degenerate = oldStart == oldEnd;
		// Normalize at the original start, then move. Normalization itself is
		// not counted, and a failed movement must leave the original range alone.
		auto position = degenerate ? oldStart : UnitRange(editor, oldStart, supported).start;
		const auto normalizedStart = position;
		auto moved = MovePosition(editor, supported, count, position);
		if (!degenerate && position == length && moved > 0)
		{
			const auto finalCharacter = supported == TextUnit::Character
				? MoveCharacters(editor, length, -1) : editor.PositionBefore(length);
			position = UnitRange(editor, finalCharacter, supported).start;
			if (supported == TextUnit::Character)
				moved = static_cast<int32_t>(editor.CountCharacters(normalizedStart, position));
			else
				--moved;
		}
		if (moved != 0)
		{
			const auto range = degenerate ? UnitSpan{position, position} : UnitRange(editor, position, supported);
			_rangeStart = range.start;
			_rangeEnd = range.end;
		}
		return moved;
	}

	int32_t TextRangeProvider::MoveEndpointByUnit(TextPatternRangeEndpoint const &endpoint, TextUnit const &unit, int32_t count)
	{
		if (count == 0)
			return 0;
		const auto editor{ _editor.get() };
		if (!editor)
			throw hresult_error{ E_POINTER };
		const auto length{ editor.Length() };
		const auto oldPosition{ std::clamp(endpoint == TextPatternRangeEndpoint::Start ? _rangeStart : _rangeEnd,
			int64_t{ 0 }, length) };
		auto newPosition = oldPosition;
		const auto moved = MovePosition(editor, SupportedUnit(unit), count, newPosition);
		if (moved == 0)
			return 0;
		if (endpoint == TextPatternRangeEndpoint::Start)
		{
			_rangeStart = newPosition;
			_rangeEnd = std::max(_rangeStart, std::clamp(_rangeEnd, int64_t{ 0 }, length));
		}
		else
		{
			_rangeEnd = newPosition;
			_rangeStart = std::min(_rangeEnd, std::clamp(_rangeStart, int64_t{ 0 }, length));
		}
		return moved;
	}

	void TextRangeProvider::MoveEndpointByRange(TextPatternRangeEndpoint const &endpoint, ITextRangeProvider const &textRangeProvider, TextPatternRangeEndpoint const &targetEndpoint)
	{
		const auto target{ textRangeProvider.as<TextRangeProvider>() };
		const auto src{ targetEndpoint == TextPatternRangeEndpoint::Start ? target->_rangeStart : target->_rangeEnd };
		if (endpoint == TextPatternRangeEndpoint::Start)
		{
			_rangeStart = src;
			if (_rangeStart > _rangeEnd)
			{
				_rangeEnd = _rangeStart;
			}
		}
		else
		{
			_rangeEnd = src;
			if (_rangeStart > _rangeEnd)
			{
				_rangeStart = _rangeEnd;
			}
		}
	}

	void TextRangeProvider::Select()
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		editor.SetSelection(_rangeEnd, _rangeStart);
	}

	void TextRangeProvider::AddToSelection()
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		editor.AddSelection(_rangeEnd, _rangeStart);
	}

	void TextRangeProvider::RemoveFromSelection()
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		const auto count{ editor.Selections() };
		for (auto i{ 0 }; i < count; i++)
		{
			if (_rangeStart == editor.GetSelectionNStart(i)
				&& _rangeEnd == editor.GetSelectionNEnd(i))
			{
				editor.DropSelectionN(i);
				return;
			}
		}
	}

	void TextRangeProvider::ScrollIntoView(bool alignToTop)
	{
		const auto editor{ _editor.get() };
		if (!editor)
		{
			throw hresult_error{ E_POINTER };
		}

		// Todo: You would want primary to be the caret and secondary to be anchor rather than start and end
		// Todo: see if we can honor alignToTop
		editor.ScrollRange(_rangeEnd, _rangeStart);
	}

	com_array<IRawElementProviderSimple> TextRangeProvider::GetChildren()
	{
		return {};
	}
}
