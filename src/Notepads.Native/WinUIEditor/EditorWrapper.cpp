// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "EditorBaseControl.h"
#include "EditorWrapper.h"
#include "Editor.g.cpp"

namespace winrt::WinUIEditor::implementation
{
	StyleNeededEventArgs::StyleNeededEventArgs(int32_t position)
		: _position { position }
	{
	}

	int32_t StyleNeededEventArgs::Position()
	{
		return _position;
	}

	CharAddedEventArgs::CharAddedEventArgs(int32_t ch, int32_t characterSource)
		: _ch { ch }, _characterSource { characterSource }
	{
	}

	int32_t CharAddedEventArgs::Ch()
	{
		return _ch;
	}

	int32_t CharAddedEventArgs::CharacterSource()
	{
		return _characterSource;
	}

	KeyEventArgs::KeyEventArgs(int32_t ch, int32_t modifiers)
		: _ch { ch }, _modifiers { modifiers }
	{
	}

	int32_t KeyEventArgs::Ch()
	{
		return _ch;
	}

	int32_t KeyEventArgs::Modifiers()
	{
		return _modifiers;
	}

	DoubleClickEventArgs::DoubleClickEventArgs(int32_t modifiers, int32_t position, int32_t line)
		: _modifiers { modifiers }, _position { position }, _line { line }
	{
	}

	int32_t DoubleClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t DoubleClickEventArgs::Position()
	{
		return _position;
	}

	int32_t DoubleClickEventArgs::Line()
	{
		return _line;
	}

	UpdateUIEventArgs::UpdateUIEventArgs(int32_t updated)
		: _updated { updated }
	{
	}

	int32_t UpdateUIEventArgs::Updated()
	{
		return _updated;
	}

	ModifiedEventArgs::ModifiedEventArgs(int32_t position, int32_t modificationType, const char *text, int32_t length, int32_t linesAdded, int32_t line, int32_t foldLevelNow, int32_t foldLevelPrev, int32_t token, int32_t annotationLinesAdded)
		: _position { position }, _modificationType { modificationType }, _textAsPointer { text }, _length { length }, _linesAdded { linesAdded }, _line { line }, _foldLevelNow { foldLevelNow }, _foldLevelPrev { foldLevelPrev }, _token { token }, _annotationLinesAdded { annotationLinesAdded }
	{
	}

	int32_t ModifiedEventArgs::Position()
	{
		return _position;
	}

	int32_t ModifiedEventArgs::ModificationType()
	{
		return _modificationType;
	}

	Windows::Storage::Streams::IBuffer ModifiedEventArgs::TextAsBuffer()
	{
		std::lock_guard guard(_textMutex);
		if (!_textAsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_textAsBuffer)
		{
			if (_textAsPointer)
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ static_cast<uint32_t>(_length) };
				memcpy(_textAsBuffer.data(), _textAsPointer, _length);
				_textAsBuffer.Length(_length);
			}
			else
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ 0u };
			}
		}
		return _textAsBuffer;
	}

	hstring ModifiedEventArgs::Text()
	{
		std::lock_guard guard(_textMutex);
		if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_text)
		{
			_text = _textAsPointer ? to_hstring(std::string_view{ _textAsPointer, static_cast<size_t>(_length) }) : L"";
		}
		return _text.GetString();
	}

	int32_t ModifiedEventArgs::Length()
	{
		return _length;
	}

	int32_t ModifiedEventArgs::LinesAdded()
	{
		return _linesAdded;
	}

	int32_t ModifiedEventArgs::Line()
	{
		return _line;
	}

	int32_t ModifiedEventArgs::FoldLevelNow()
	{
		return _foldLevelNow;
	}

	int32_t ModifiedEventArgs::FoldLevelPrev()
	{
		return _foldLevelPrev;
	}

	int32_t ModifiedEventArgs::Token()
	{
		return _token;
	}

	int32_t ModifiedEventArgs::AnnotationLinesAdded()
	{
		return _annotationLinesAdded;
	}

	MacroRecordEventArgs::MacroRecordEventArgs(int32_t message, int32_t wParam, int32_t lParam)
		: _message { message }, _wParam { wParam }, _lParam { lParam }
	{
	}

	int32_t MacroRecordEventArgs::Message()
	{
		return _message;
	}

	int32_t MacroRecordEventArgs::WParam()
	{
		return _wParam;
	}

	int32_t MacroRecordEventArgs::LParam()
	{
		return _lParam;
	}

	MarginClickEventArgs::MarginClickEventArgs(int32_t modifiers, int32_t position, int32_t margin)
		: _modifiers { modifiers }, _position { position }, _margin { margin }
	{
	}

	int32_t MarginClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t MarginClickEventArgs::Position()
	{
		return _position;
	}

	int32_t MarginClickEventArgs::Margin()
	{
		return _margin;
	}

	NeedShownEventArgs::NeedShownEventArgs(int32_t position, int32_t length)
		: _position { position }, _length { length }
	{
	}

	int32_t NeedShownEventArgs::Position()
	{
		return _position;
	}

	int32_t NeedShownEventArgs::Length()
	{
		return _length;
	}

	UserListSelectionEventArgs::UserListSelectionEventArgs(int32_t listType, const char *text, int32_t position, int32_t ch, WinUIEditor::CompletionMethods const &listCompletionMethod)
		: _listType { listType }, _textAsPointer { text }, _position { position }, _ch { ch }, _listCompletionMethod { listCompletionMethod }
	{
	}

	int32_t UserListSelectionEventArgs::ListType()
	{
		return _listType;
	}

	Windows::Storage::Streams::IBuffer UserListSelectionEventArgs::TextAsBuffer()
	{
		std::lock_guard guard(_textMutex);
		if (!_textAsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_textAsBuffer)
		{
			if (_textAsPointer)
			{
				const auto length { strnlen_s(_textAsPointer, std::numeric_limits<uint32_t>::max()) };
				_textAsBuffer = Windows::Storage::Streams::Buffer{ static_cast<uint32_t>(length) };
				memcpy(_textAsBuffer.data(), _textAsPointer, length);
				_textAsBuffer.Length(length);
			}
			else
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ 0u };
			}
		}
		return _textAsBuffer;
	}

	hstring UserListSelectionEventArgs::Text()
	{
		std::lock_guard guard(_textMutex);
		if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_text)
		{
			_text = _textAsPointer ? to_hstring(_textAsPointer) : L"";
		}
		return _text.GetString();
	}

	int32_t UserListSelectionEventArgs::Position()
	{
		return _position;
	}

	int32_t UserListSelectionEventArgs::Ch()
	{
		return _ch;
	}

	WinUIEditor::CompletionMethods UserListSelectionEventArgs::ListCompletionMethod()
	{
		return _listCompletionMethod;
	}

	URIDroppedEventArgs::URIDroppedEventArgs(const char *text)
		: _textAsPointer { text }
	{
	}

	Windows::Storage::Streams::IBuffer URIDroppedEventArgs::TextAsBuffer()
	{
		std::lock_guard guard(_textMutex);
		if (!_textAsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_textAsBuffer)
		{
			if (_textAsPointer)
			{
				const auto length { strnlen_s(_textAsPointer, std::numeric_limits<uint32_t>::max()) };
				_textAsBuffer = Windows::Storage::Streams::Buffer{ static_cast<uint32_t>(length) };
				memcpy(_textAsBuffer.data(), _textAsPointer, length);
				_textAsBuffer.Length(length);
			}
			else
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ 0u };
			}
		}
		return _textAsBuffer;
	}

	hstring URIDroppedEventArgs::Text()
	{
		std::lock_guard guard(_textMutex);
		if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_text)
		{
			_text = _textAsPointer ? to_hstring(_textAsPointer) : L"";
		}
		return _text.GetString();
	}

	DwellStartEventArgs::DwellStartEventArgs(int32_t position, int32_t x, int32_t y)
		: _position { position }, _x { x }, _y { y }
	{
	}

	int32_t DwellStartEventArgs::Position()
	{
		return _position;
	}

	int32_t DwellStartEventArgs::X()
	{
		return _x;
	}

	int32_t DwellStartEventArgs::Y()
	{
		return _y;
	}

	DwellEndEventArgs::DwellEndEventArgs(int32_t position, int32_t x, int32_t y)
		: _position { position }, _x { x }, _y { y }
	{
	}

	int32_t DwellEndEventArgs::Position()
	{
		return _position;
	}

	int32_t DwellEndEventArgs::X()
	{
		return _x;
	}

	int32_t DwellEndEventArgs::Y()
	{
		return _y;
	}

	HotSpotClickEventArgs::HotSpotClickEventArgs(int32_t modifiers, int32_t position)
		: _modifiers { modifiers }, _position { position }
	{
	}

	int32_t HotSpotClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t HotSpotClickEventArgs::Position()
	{
		return _position;
	}

	HotSpotDoubleClickEventArgs::HotSpotDoubleClickEventArgs(int32_t modifiers, int32_t position)
		: _modifiers { modifiers }, _position { position }
	{
	}

	int32_t HotSpotDoubleClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t HotSpotDoubleClickEventArgs::Position()
	{
		return _position;
	}

	CallTipClickEventArgs::CallTipClickEventArgs(int32_t position)
		: _position { position }
	{
	}

	int32_t CallTipClickEventArgs::Position()
	{
		return _position;
	}

	AutoCSelectionEventArgs::AutoCSelectionEventArgs(const char *text, int32_t position, int32_t ch, WinUIEditor::CompletionMethods const &listCompletionMethod)
		: _textAsPointer { text }, _position { position }, _ch { ch }, _listCompletionMethod { listCompletionMethod }
	{
	}

	Windows::Storage::Streams::IBuffer AutoCSelectionEventArgs::TextAsBuffer()
	{
		std::lock_guard guard(_textMutex);
		if (!_textAsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_textAsBuffer)
		{
			if (_textAsPointer)
			{
				const auto length { strnlen_s(_textAsPointer, std::numeric_limits<uint32_t>::max()) };
				_textAsBuffer = Windows::Storage::Streams::Buffer{ static_cast<uint32_t>(length) };
				memcpy(_textAsBuffer.data(), _textAsPointer, length);
				_textAsBuffer.Length(length);
			}
			else
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ 0u };
			}
		}
		return _textAsBuffer;
	}

	hstring AutoCSelectionEventArgs::Text()
	{
		std::lock_guard guard(_textMutex);
		if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_text)
		{
			_text = _textAsPointer ? to_hstring(_textAsPointer) : L"";
		}
		return _text.GetString();
	}

	int32_t AutoCSelectionEventArgs::Position()
	{
		return _position;
	}

	int32_t AutoCSelectionEventArgs::Ch()
	{
		return _ch;
	}

	WinUIEditor::CompletionMethods AutoCSelectionEventArgs::ListCompletionMethod()
	{
		return _listCompletionMethod;
	}

	IndicatorClickEventArgs::IndicatorClickEventArgs(int32_t modifiers, int32_t position)
		: _modifiers { modifiers }, _position { position }
	{
	}

	int32_t IndicatorClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t IndicatorClickEventArgs::Position()
	{
		return _position;
	}

	IndicatorReleaseEventArgs::IndicatorReleaseEventArgs(int32_t modifiers, int32_t position)
		: _modifiers { modifiers }, _position { position }
	{
	}

	int32_t IndicatorReleaseEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t IndicatorReleaseEventArgs::Position()
	{
		return _position;
	}

	HotSpotReleaseClickEventArgs::HotSpotReleaseClickEventArgs(int32_t modifiers, int32_t position)
		: _modifiers { modifiers }, _position { position }
	{
	}

	int32_t HotSpotReleaseClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t HotSpotReleaseClickEventArgs::Position()
	{
		return _position;
	}

	AutoCCompletedEventArgs::AutoCCompletedEventArgs(const char *text, int32_t position, int32_t ch, WinUIEditor::CompletionMethods const &listCompletionMethod)
		: _textAsPointer { text }, _position { position }, _ch { ch }, _listCompletionMethod { listCompletionMethod }
	{
	}

	Windows::Storage::Streams::IBuffer AutoCCompletedEventArgs::TextAsBuffer()
	{
		std::lock_guard guard(_textMutex);
		if (!_textAsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_textAsBuffer)
		{
			if (_textAsPointer)
			{
				const auto length { strnlen_s(_textAsPointer, std::numeric_limits<uint32_t>::max()) };
				_textAsBuffer = Windows::Storage::Streams::Buffer{ static_cast<uint32_t>(length) };
				memcpy(_textAsBuffer.data(), _textAsPointer, length);
				_textAsBuffer.Length(length);
			}
			else
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ 0u };
			}
		}
		return _textAsBuffer;
	}

	hstring AutoCCompletedEventArgs::Text()
	{
		std::lock_guard guard(_textMutex);
		if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_text)
		{
			_text = _textAsPointer ? to_hstring(_textAsPointer) : L"";
		}
		return _text.GetString();
	}

	int32_t AutoCCompletedEventArgs::Position()
	{
		return _position;
	}

	int32_t AutoCCompletedEventArgs::Ch()
	{
		return _ch;
	}

	WinUIEditor::CompletionMethods AutoCCompletedEventArgs::ListCompletionMethod()
	{
		return _listCompletionMethod;
	}

	MarginRightClickEventArgs::MarginRightClickEventArgs(int32_t modifiers, int32_t position, int32_t margin)
		: _modifiers { modifiers }, _position { position }, _margin { margin }
	{
	}

	int32_t MarginRightClickEventArgs::Modifiers()
	{
		return _modifiers;
	}

	int32_t MarginRightClickEventArgs::Position()
	{
		return _position;
	}

	int32_t MarginRightClickEventArgs::Margin()
	{
		return _margin;
	}

	AutoCSelectionChangeEventArgs::AutoCSelectionChangeEventArgs(int32_t listType, const char *text, int32_t position)
		: _listType { listType }, _textAsPointer { text }, _position { position }
	{
	}

	int32_t AutoCSelectionChangeEventArgs::ListType()
	{
		return _listType;
	}

	Windows::Storage::Streams::IBuffer AutoCSelectionChangeEventArgs::TextAsBuffer()
	{
		std::lock_guard guard(_textMutex);
		if (!_textAsBuffer && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_textAsBuffer)
		{
			if (_textAsPointer)
			{
				const auto length { strnlen_s(_textAsPointer, std::numeric_limits<uint32_t>::max()) };
				_textAsBuffer = Windows::Storage::Streams::Buffer{ static_cast<uint32_t>(length) };
				memcpy(_textAsBuffer.data(), _textAsPointer, length);
				_textAsBuffer.Length(length);
			}
			else
			{
				_textAsBuffer = Windows::Storage::Streams::Buffer{ 0u };
			}
		}
		return _textAsBuffer;
	}

	hstring AutoCSelectionChangeEventArgs::Text()
	{
		std::lock_guard guard(_textMutex);
		if (!_text && _textExpired) throw_hresult(RO_E_CLOSED);
		if (!_text)
		{
			_text = _textAsPointer ? to_hstring(_textAsPointer) : L"";
		}
		return _text.GetString();
	}

	int32_t AutoCSelectionChangeEventArgs::Position()
	{
		return _position;
	}

	Editor::Editor(com_ptr<EditorBaseControl> const &editor)
		: _editor{ editor }
	{
	}

	void Editor::ProcessEvent(Scintilla::NotificationData *data)
	{
		switch (static_cast<int>(data->nmhdr.code))
		{
		case 2000: // StyleNeeded
		{
			if (_styleNeededEvent)
			{
				_styleNeededEvent(*this, *make_self<implementation::StyleNeededEventArgs>(static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2001: // CharAdded
		{
			if (_charAddedEvent)
			{
				_charAddedEvent(*this, *make_self<implementation::CharAddedEventArgs>(static_cast<int32_t>(data->ch), static_cast<int32_t>(data->characterSource)));
			}
		}
		break;
		case 2002: // SavePointReached
		{
			if (_savePointReachedEvent)
			{
				_savePointReachedEvent(*this, *make_self<implementation::SavePointReachedEventArgs>());
			}
		}
		break;
		case 2003: // SavePointLeft
		{
			if (_savePointLeftEvent)
			{
				_savePointLeftEvent(*this, *make_self<implementation::SavePointLeftEventArgs>());
			}
		}
		break;
		case 2004: // ModifyAttemptRO
		{
			if (_modifyAttemptROEvent)
			{
				_modifyAttemptROEvent(*this, *make_self<implementation::ModifyAttemptROEventArgs>());
			}
		}
		break;
		case 2005: // Key
		{
			if (_keyEvent)
			{
				_keyEvent(*this, *make_self<implementation::KeyEventArgs>(static_cast<int32_t>(data->ch), static_cast<int32_t>(data->modifiers)));
			}
		}
		break;
		case 2006: // DoubleClick
		{
			if (_doubleClickEvent)
			{
				_doubleClickEvent(*this, *make_self<implementation::DoubleClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position), static_cast<int32_t>(data->line)));
			}
		}
		break;
		case 2007: // UpdateUI
		{
			if (_updateUIEvent)
			{
				_updateUIEvent(*this, *make_self<implementation::UpdateUIEventArgs>(static_cast<int32_t>(data->updated)));
			}
		}
		break;
		case 2008: // Modified
		{
			if (_modifiedEvent)
			{
				auto args = make_self<implementation::ModifiedEventArgs>(static_cast<int32_t>(data->position), static_cast<int32_t>(data->modificationType), data->text, static_cast<int32_t>(data->length), static_cast<int32_t>(data->linesAdded), static_cast<int32_t>(data->line), static_cast<int32_t>(data->foldLevelNow), static_cast<int32_t>(data->foldLevelPrev), static_cast<int32_t>(data->token), static_cast<int32_t>(data->annotationLinesAdded));
				struct TextLifetime { implementation::ModifiedEventArgs *args; ~TextLifetime() { args->InvalidateText(); } } lifetime{ args.get() };
				_modifiedEvent(*this, *args);
			}
		}
		break;
		case 2009: // MacroRecord
		{
			if (_macroRecordEvent)
			{
				_macroRecordEvent(*this, *make_self<implementation::MacroRecordEventArgs>(static_cast<int32_t>(data->message), static_cast<int32_t>(data->wParam), static_cast<int32_t>(data->lParam)));
			}
		}
		break;
		case 2010: // MarginClick
		{
			if (_marginClickEvent)
			{
				_marginClickEvent(*this, *make_self<implementation::MarginClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position), static_cast<int32_t>(data->margin)));
			}
		}
		break;
		case 2011: // NeedShown
		{
			if (_needShownEvent)
			{
				_needShownEvent(*this, *make_self<implementation::NeedShownEventArgs>(static_cast<int32_t>(data->position), static_cast<int32_t>(data->length)));
			}
		}
		break;
		case 2013: // Painted
		{
			if (_paintedEvent)
			{
				_paintedEvent(*this, *make_self<implementation::PaintedEventArgs>());
			}
		}
		break;
		case 2014: // UserListSelection
		{
			if (_userListSelectionEvent)
			{
				auto args = make_self<implementation::UserListSelectionEventArgs>(static_cast<int32_t>(data->listType), data->text, static_cast<int32_t>(data->position), static_cast<int32_t>(data->ch), static_cast<WinUIEditor::CompletionMethods>(data->listCompletionMethod));
				struct TextLifetime { implementation::UserListSelectionEventArgs *args; ~TextLifetime() { args->InvalidateText(); } } lifetime{ args.get() };
				_userListSelectionEvent(*this, *args);
			}
		}
		break;
		case 2015: // URIDropped
		{
			if (_uRIDroppedEvent)
			{
				auto args = make_self<implementation::URIDroppedEventArgs>(data->text);
				struct TextLifetime { implementation::URIDroppedEventArgs *args; ~TextLifetime() { args->InvalidateText(); } } lifetime{ args.get() };
				_uRIDroppedEvent(*this, *args);
			}
		}
		break;
		case 2016: // DwellStart
		{
			if (_dwellStartEvent)
			{
				_dwellStartEvent(*this, *make_self<implementation::DwellStartEventArgs>(static_cast<int32_t>(data->position), static_cast<int32_t>(data->x), static_cast<int32_t>(data->y)));
			}
		}
		break;
		case 2017: // DwellEnd
		{
			if (_dwellEndEvent)
			{
				_dwellEndEvent(*this, *make_self<implementation::DwellEndEventArgs>(static_cast<int32_t>(data->position), static_cast<int32_t>(data->x), static_cast<int32_t>(data->y)));
			}
		}
		break;
		case 2018: // ZoomChanged
		{
			if (_zoomChangedEvent)
			{
				_zoomChangedEvent(*this, *make_self<implementation::ZoomChangedEventArgs>());
			}
		}
		break;
		case 2019: // HotSpotClick
		{
			if (_hotSpotClickEvent)
			{
				_hotSpotClickEvent(*this, *make_self<implementation::HotSpotClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2020: // HotSpotDoubleClick
		{
			if (_hotSpotDoubleClickEvent)
			{
				_hotSpotDoubleClickEvent(*this, *make_self<implementation::HotSpotDoubleClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2021: // CallTipClick
		{
			if (_callTipClickEvent)
			{
				_callTipClickEvent(*this, *make_self<implementation::CallTipClickEventArgs>(static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2022: // AutoCSelection
		{
			if (_autoCSelectionEvent)
			{
				auto args = make_self<implementation::AutoCSelectionEventArgs>(data->text, static_cast<int32_t>(data->position), static_cast<int32_t>(data->ch), static_cast<WinUIEditor::CompletionMethods>(data->listCompletionMethod));
				struct TextLifetime { implementation::AutoCSelectionEventArgs *args; ~TextLifetime() { args->InvalidateText(); } } lifetime{ args.get() };
				_autoCSelectionEvent(*this, *args);
			}
		}
		break;
		case 2023: // IndicatorClick
		{
			if (_indicatorClickEvent)
			{
				_indicatorClickEvent(*this, *make_self<implementation::IndicatorClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2024: // IndicatorRelease
		{
			if (_indicatorReleaseEvent)
			{
				_indicatorReleaseEvent(*this, *make_self<implementation::IndicatorReleaseEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2025: // AutoCCancelled
		{
			if (_autoCCancelledEvent)
			{
				_autoCCancelledEvent(*this, *make_self<implementation::AutoCCancelledEventArgs>());
			}
		}
		break;
		case 2026: // AutoCCharDeleted
		{
			if (_autoCCharDeletedEvent)
			{
				_autoCCharDeletedEvent(*this, *make_self<implementation::AutoCCharDeletedEventArgs>());
			}
		}
		break;
		case 2027: // HotSpotReleaseClick
		{
			if (_hotSpotReleaseClickEvent)
			{
				_hotSpotReleaseClickEvent(*this, *make_self<implementation::HotSpotReleaseClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position)));
			}
		}
		break;
		case 2028: // FocusIn
		{
			if (_focusInEvent)
			{
				_focusInEvent(*this, *make_self<implementation::FocusInEventArgs>());
			}
		}
		break;
		case 2029: // FocusOut
		{
			if (_focusOutEvent)
			{
				_focusOutEvent(*this, *make_self<implementation::FocusOutEventArgs>());
			}
		}
		break;
		case 2030: // AutoCCompleted
		{
			if (_autoCCompletedEvent)
			{
				auto args = make_self<implementation::AutoCCompletedEventArgs>(data->text, static_cast<int32_t>(data->position), static_cast<int32_t>(data->ch), static_cast<WinUIEditor::CompletionMethods>(data->listCompletionMethod));
				struct TextLifetime { implementation::AutoCCompletedEventArgs *args; ~TextLifetime() { args->InvalidateText(); } } lifetime{ args.get() };
				_autoCCompletedEvent(*this, *args);
			}
		}
		break;
		case 2031: // MarginRightClick
		{
			if (_marginRightClickEvent)
			{
				_marginRightClickEvent(*this, *make_self<implementation::MarginRightClickEventArgs>(static_cast<int32_t>(data->modifiers), static_cast<int32_t>(data->position), static_cast<int32_t>(data->margin)));
			}
		}
		break;
		case 2032: // AutoCSelectionChange
		{
			if (_autoCSelectionChangeEvent)
			{
				auto args = make_self<implementation::AutoCSelectionChangeEventArgs>(static_cast<int32_t>(data->listType), data->text, static_cast<int32_t>(data->position));
				struct TextLifetime { implementation::AutoCSelectionChangeEventArgs *args; ~TextLifetime() { args->InvalidateText(); } } lifetime{ args.get() };
				_autoCSelectionChangeEvent(*this, *args);
			}
		}
		break;
		}
	}

	event_token Editor::StyleNeeded(WinUIEditor::StyleNeededHandler const &handler)
	{
		return _styleNeededEvent.add(handler);
	}

	void Editor::StyleNeeded(event_token const &token)
	{
		_styleNeededEvent.remove(token);
	}

	event_token Editor::CharAdded(WinUIEditor::CharAddedHandler const &handler)
	{
		return _charAddedEvent.add(handler);
	}

	void Editor::CharAdded(event_token const &token)
	{
		_charAddedEvent.remove(token);
	}

	event_token Editor::SavePointReached(WinUIEditor::SavePointReachedHandler const &handler)
	{
		return _savePointReachedEvent.add(handler);
	}

	void Editor::SavePointReached(event_token const &token)
	{
		_savePointReachedEvent.remove(token);
	}

	event_token Editor::SavePointLeft(WinUIEditor::SavePointLeftHandler const &handler)
	{
		return _savePointLeftEvent.add(handler);
	}

	void Editor::SavePointLeft(event_token const &token)
	{
		_savePointLeftEvent.remove(token);
	}

	event_token Editor::ModifyAttemptRO(WinUIEditor::ModifyAttemptROHandler const &handler)
	{
		return _modifyAttemptROEvent.add(handler);
	}

	void Editor::ModifyAttemptRO(event_token const &token)
	{
		_modifyAttemptROEvent.remove(token);
	}

	event_token Editor::Key(WinUIEditor::KeyHandler const &handler)
	{
		return _keyEvent.add(handler);
	}

	void Editor::Key(event_token const &token)
	{
		_keyEvent.remove(token);
	}

	event_token Editor::DoubleClick(WinUIEditor::DoubleClickHandler const &handler)
	{
		return _doubleClickEvent.add(handler);
	}

	void Editor::DoubleClick(event_token const &token)
	{
		_doubleClickEvent.remove(token);
	}

	event_token Editor::UpdateUI(WinUIEditor::UpdateUIHandler const &handler)
	{
		return _updateUIEvent.add(handler);
	}

	void Editor::UpdateUI(event_token const &token)
	{
		_updateUIEvent.remove(token);
	}

	event_token Editor::Modified(WinUIEditor::ModifiedHandler const &handler)
	{
		return _modifiedEvent.add(handler);
	}

	void Editor::Modified(event_token const &token)
	{
		_modifiedEvent.remove(token);
	}

	event_token Editor::MacroRecord(WinUIEditor::MacroRecordHandler const &handler)
	{
		return _macroRecordEvent.add(handler);
	}

	void Editor::MacroRecord(event_token const &token)
	{
		_macroRecordEvent.remove(token);
	}

	event_token Editor::MarginClick(WinUIEditor::MarginClickHandler const &handler)
	{
		return _marginClickEvent.add(handler);
	}

	void Editor::MarginClick(event_token const &token)
	{
		_marginClickEvent.remove(token);
	}

	event_token Editor::NeedShown(WinUIEditor::NeedShownHandler const &handler)
	{
		return _needShownEvent.add(handler);
	}

	void Editor::NeedShown(event_token const &token)
	{
		_needShownEvent.remove(token);
	}

	event_token Editor::Painted(WinUIEditor::PaintedHandler const &handler)
	{
		return _paintedEvent.add(handler);
	}

	void Editor::Painted(event_token const &token)
	{
		_paintedEvent.remove(token);
	}

	event_token Editor::UserListSelection(WinUIEditor::UserListSelectionHandler const &handler)
	{
		return _userListSelectionEvent.add(handler);
	}

	void Editor::UserListSelection(event_token const &token)
	{
		_userListSelectionEvent.remove(token);
	}

	event_token Editor::URIDropped(WinUIEditor::URIDroppedHandler const &handler)
	{
		return _uRIDroppedEvent.add(handler);
	}

	void Editor::URIDropped(event_token const &token)
	{
		_uRIDroppedEvent.remove(token);
	}

	event_token Editor::DwellStart(WinUIEditor::DwellStartHandler const &handler)
	{
		return _dwellStartEvent.add(handler);
	}

	void Editor::DwellStart(event_token const &token)
	{
		_dwellStartEvent.remove(token);
	}

	event_token Editor::DwellEnd(WinUIEditor::DwellEndHandler const &handler)
	{
		return _dwellEndEvent.add(handler);
	}

	void Editor::DwellEnd(event_token const &token)
	{
		_dwellEndEvent.remove(token);
	}

	event_token Editor::ZoomChanged(WinUIEditor::ZoomChangedHandler const &handler)
	{
		return _zoomChangedEvent.add(handler);
	}

	void Editor::ZoomChanged(event_token const &token)
	{
		_zoomChangedEvent.remove(token);
	}

	event_token Editor::HotSpotClick(WinUIEditor::HotSpotClickHandler const &handler)
	{
		return _hotSpotClickEvent.add(handler);
	}

	void Editor::HotSpotClick(event_token const &token)
	{
		_hotSpotClickEvent.remove(token);
	}

	event_token Editor::HotSpotDoubleClick(WinUIEditor::HotSpotDoubleClickHandler const &handler)
	{
		return _hotSpotDoubleClickEvent.add(handler);
	}

	void Editor::HotSpotDoubleClick(event_token const &token)
	{
		_hotSpotDoubleClickEvent.remove(token);
	}

	event_token Editor::CallTipClick(WinUIEditor::CallTipClickHandler const &handler)
	{
		return _callTipClickEvent.add(handler);
	}

	void Editor::CallTipClick(event_token const &token)
	{
		_callTipClickEvent.remove(token);
	}

	event_token Editor::AutoCSelection(WinUIEditor::AutoCSelectionHandler const &handler)
	{
		return _autoCSelectionEvent.add(handler);
	}

	void Editor::AutoCSelection(event_token const &token)
	{
		_autoCSelectionEvent.remove(token);
	}

	event_token Editor::IndicatorClick(WinUIEditor::IndicatorClickHandler const &handler)
	{
		return _indicatorClickEvent.add(handler);
	}

	void Editor::IndicatorClick(event_token const &token)
	{
		_indicatorClickEvent.remove(token);
	}

	event_token Editor::IndicatorRelease(WinUIEditor::IndicatorReleaseHandler const &handler)
	{
		return _indicatorReleaseEvent.add(handler);
	}

	void Editor::IndicatorRelease(event_token const &token)
	{
		_indicatorReleaseEvent.remove(token);
	}

	event_token Editor::AutoCCancelled(WinUIEditor::AutoCCancelledHandler const &handler)
	{
		return _autoCCancelledEvent.add(handler);
	}

	void Editor::AutoCCancelled(event_token const &token)
	{
		_autoCCancelledEvent.remove(token);
	}

	event_token Editor::AutoCCharDeleted(WinUIEditor::AutoCCharDeletedHandler const &handler)
	{
		return _autoCCharDeletedEvent.add(handler);
	}

	void Editor::AutoCCharDeleted(event_token const &token)
	{
		_autoCCharDeletedEvent.remove(token);
	}

	event_token Editor::HotSpotReleaseClick(WinUIEditor::HotSpotReleaseClickHandler const &handler)
	{
		return _hotSpotReleaseClickEvent.add(handler);
	}

	void Editor::HotSpotReleaseClick(event_token const &token)
	{
		_hotSpotReleaseClickEvent.remove(token);
	}

	event_token Editor::FocusIn(WinUIEditor::FocusInHandler const &handler)
	{
		return _focusInEvent.add(handler);
	}

	void Editor::FocusIn(event_token const &token)
	{
		_focusInEvent.remove(token);
	}

	event_token Editor::FocusOut(WinUIEditor::FocusOutHandler const &handler)
	{
		return _focusOutEvent.add(handler);
	}

	void Editor::FocusOut(event_token const &token)
	{
		_focusOutEvent.remove(token);
	}

	event_token Editor::AutoCCompleted(WinUIEditor::AutoCCompletedHandler const &handler)
	{
		return _autoCCompletedEvent.add(handler);
	}

	void Editor::AutoCCompleted(event_token const &token)
	{
		_autoCCompletedEvent.remove(token);
	}

	event_token Editor::MarginRightClick(WinUIEditor::MarginRightClickHandler const &handler)
	{
		return _marginRightClickEvent.add(handler);
	}

	void Editor::MarginRightClick(event_token const &token)
	{
		_marginRightClickEvent.remove(token);
	}

	event_token Editor::AutoCSelectionChange(WinUIEditor::AutoCSelectionChangeHandler const &handler)
	{
		return _autoCSelectionChangeEvent.add(handler);
	}

	void Editor::AutoCSelectionChange(event_token const &token)
	{
		_autoCSelectionChangeEvent.remove(token);
	}

	/**
	 * Returns the number of bytes in the document.
	 */
	int64_t Editor::Length()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLength, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the position of the caret.
	 */
	int64_t Editor::CurrentPos()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCurrentPos, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the position of the caret.
	 */
	void Editor::CurrentPos(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCurrentPos, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the position of the opposite end of the selection to the caret.
	 */
	int64_t Editor::Anchor()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetAnchor, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the selection anchor to a position. The anchor is the opposite
	 * end of the selection from the caret.
	 */
	void Editor::Anchor(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAnchor, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is undo history being collected?
	 */
	bool Editor::UndoCollection()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoCollection, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Choose between collecting actions into the undo
	 * history and discarding them.
	 */
	void Editor::UndoCollection(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUndoCollection, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Are white space characters currently visible?
	 * Returns one of SCWS_* constants.
	 */
	WinUIEditor::WhiteSpace Editor::ViewWS()
	{
		return static_cast<WinUIEditor::WhiteSpace>(_editor.get()->PublicWndProc(Scintilla::Message::GetViewWS, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Make white space characters invisible, always visible or visible outside indentation.
	 */
	void Editor::ViewWS(WinUIEditor::WhiteSpace const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetViewWS, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the current tab draw mode.
	 * Returns one of SCTD_* constants.
	 */
	WinUIEditor::TabDrawMode Editor::TabDrawMode()
	{
		return static_cast<WinUIEditor::TabDrawMode>(_editor.get()->PublicWndProc(Scintilla::Message::GetTabDrawMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set how tabs are drawn when visible.
	 */
	void Editor::TabDrawMode(WinUIEditor::TabDrawMode const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTabDrawMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the position of the last correctly styled character.
	 */
	int64_t Editor::EndStyled()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetEndStyled, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the current end of line mode - one of CRLF, CR, or LF.
	 */
	WinUIEditor::EndOfLine Editor::EOLMode()
	{
		return static_cast<WinUIEditor::EndOfLine>(_editor.get()->PublicWndProc(Scintilla::Message::GetEOLMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the current end of line mode.
	 */
	void Editor::EOLMode(WinUIEditor::EndOfLine const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetEOLMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is drawing done first into a buffer or direct to the screen?
	 */
	bool Editor::BufferedDraw()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetBufferedDraw, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * If drawing is buffered then each line of text is drawn into a bitmap buffer
	 * before drawing it to the screen to avoid flicker.
	 */
	void Editor::BufferedDraw(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetBufferedDraw, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the visible size of a tab.
	 */
	int32_t Editor::TabWidth()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTabWidth, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the visible size of a tab to be a multiple of the width of a space character.
	 */
	void Editor::TabWidth(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTabWidth, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the minimum visual width of a tab.
	 */
	int32_t Editor::TabMinimumWidth()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTabMinimumWidth, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the minimum visual width of a tab.
	 */
	void Editor::TabMinimumWidth(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTabMinimumWidth, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is the IME displayed in a window or inline?
	 */
	WinUIEditor::IMEInteraction Editor::IMEInteraction()
	{
		return static_cast<WinUIEditor::IMEInteraction>(_editor.get()->PublicWndProc(Scintilla::Message::GetIMEInteraction, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Choose to display the IME in a window or inline.
	 */
	void Editor::IMEInteraction(WinUIEditor::IMEInteraction const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIMEInteraction, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * How many margins are there?.
	 */
	int32_t Editor::Margins()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMargins, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Allocate a non-standard number of margins.
	 */
	void Editor::Margins(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMargins, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the alpha of the selection.
	 */
	WinUIEditor::Alpha Editor::SelAlpha()
	{
		return static_cast<WinUIEditor::Alpha>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelAlpha, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the alpha of the selection.
	 */
	void Editor::SelAlpha(WinUIEditor::Alpha const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelAlpha, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is the selection end of line filled?
	 */
	bool Editor::SelEOLFilled()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelEOLFilled, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the selection to have its end of line filled or not.
	 */
	void Editor::SelEOLFilled(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelEOLFilled, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the layer for drawing selections
	 */
	WinUIEditor::Layer Editor::SelectionLayer()
	{
		return static_cast<WinUIEditor::Layer>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionLayer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the layer for drawing selections: either opaquely on base layer or translucently over text
	 */
	void Editor::SelectionLayer(WinUIEditor::Layer const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionLayer, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the layer of the background of the line containing the caret.
	 */
	WinUIEditor::Layer Editor::CaretLineLayer()
	{
		return static_cast<WinUIEditor::Layer>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineLayer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the layer of the background of the line containing the caret.
	 */
	void Editor::CaretLineLayer(WinUIEditor::Layer const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineLayer, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get only highlighting subline instead of whole line.
	 */
	bool Editor::CaretLineHighlightSubLine()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineHighlightSubLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set only highlighting subline instead of whole line.
	 */
	void Editor::CaretLineHighlightSubLine(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineHighlightSubLine, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the time in milliseconds that the caret is on and off.
	 */
	int32_t Editor::CaretPeriod()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretPeriod, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the time in milliseconds that the caret is on and off. 0 = steady on.
	 */
	void Editor::CaretPeriod(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretPeriod, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the number of characters to have directly indexed categories
	 */
	int32_t Editor::CharacterCategoryOptimization()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCharacterCategoryOptimization, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the number of characters to have directly indexed categories
	 */
	void Editor::CharacterCategoryOptimization(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCharacterCategoryOptimization, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is an undo sequence active?
	 */
	int32_t Editor::UndoSequence()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoSequence, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * How many undo actions are in the history?
	 */
	int32_t Editor::UndoActions()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoActions, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Which action is the save point?
	 */
	int32_t Editor::UndoSavePoint()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoSavePoint, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set action as the save point
	 */
	void Editor::UndoSavePoint(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUndoSavePoint, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Which action is the detach point?
	 */
	int32_t Editor::UndoDetach()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoDetach, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set action as the detach point
	 */
	void Editor::UndoDetach(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUndoDetach, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Which action is the tentative point?
	 */
	int32_t Editor::UndoTentative()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoTentative, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set action as the tentative point
	 */
	void Editor::UndoTentative(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUndoTentative, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Which action is the current point?
	 */
	int32_t Editor::UndoCurrent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoCurrent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set action as the current point
	 */
	void Editor::UndoCurrent(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUndoCurrent, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the size of the dots used to mark space characters.
	 */
	int32_t Editor::WhitespaceSize()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetWhitespaceSize, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the size of the dots used to mark space characters.
	 */
	void Editor::WhitespaceSize(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWhitespaceSize, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the last line number that has line state.
	 */
	int32_t Editor::MaxLineState()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMaxLineState, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Is the background of the line containing the caret in a different colour?
	 */
	bool Editor::CaretLineVisible()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineVisible, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Display the background of the line containing the caret in a different colour.
	 */
	void Editor::CaretLineVisible(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineVisible, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the colour of the background of the line containing the caret.
	 */
	int32_t Editor::CaretLineBack()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineBack, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the colour of the background of the line containing the caret.
	 */
	void Editor::CaretLineBack(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineBack, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the caret line frame width.
	 * Width = 0 means this option is disabled.
	 */
	int32_t Editor::CaretLineFrame()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineFrame, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Display the caret line framed.
	 * Set width != 0 to enable this option and width = 0 to disable it.
	 */
	void Editor::CaretLineFrame(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineFrame, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the auto-completion list separator character.
	 */
	int32_t Editor::AutoCSeparator()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetSeparator, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the separator character in the string setting up an auto-completion list.
	 * Default is space but can be changed if items contain space.
	 */
	void Editor::AutoCSeparator(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetSeparator, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether auto-completion cancelled by backspacing before start.
	 */
	bool Editor::AutoCCancelAtStart()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetCancelAtStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Should the auto-completion list be cancelled if the user backspaces to a
	 * position before where the box was created.
	 */
	void Editor::AutoCCancelAtStart(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetCancelAtStart, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether a single item auto-completion list automatically choose the item.
	 */
	bool Editor::AutoCChooseSingle()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetChooseSingle, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Should a single item auto-completion list automatically choose the item.
	 */
	void Editor::AutoCChooseSingle(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetChooseSingle, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve state of ignore case flag.
	 */
	bool Editor::AutoCIgnoreCase()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetIgnoreCase, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether case is significant when performing auto-completion searches.
	 */
	void Editor::AutoCIgnoreCase(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetIgnoreCase, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether or not autocompletion is hidden automatically when nothing matches.
	 */
	bool Editor::AutoCAutoHide()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetAutoHide, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether or not autocompletion is hidden automatically when nothing matches.
	 */
	void Editor::AutoCAutoHide(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetAutoHide, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve autocompletion options.
	 */
	WinUIEditor::AutoCompleteOption Editor::AutoCOptions()
	{
		return static_cast<WinUIEditor::AutoCompleteOption>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetOptions, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set autocompletion options.
	 */
	void Editor::AutoCOptions(WinUIEditor::AutoCompleteOption const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetOptions, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether or not autocompletion deletes any word characters
	 * after the inserted text upon completion.
	 */
	bool Editor::AutoCDropRestOfWord()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetDropRestOfWord, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether or not autocompletion deletes any word characters
	 * after the inserted text upon completion.
	 */
	void Editor::AutoCDropRestOfWord(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetDropRestOfWord, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the auto-completion list type-separator character.
	 */
	int32_t Editor::AutoCTypeSeparator()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetTypeSeparator, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the type-separator character in the string setting up an auto-completion list.
	 * Default is '?' but can be changed if items contain '?'.
	 */
	void Editor::AutoCTypeSeparator(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetTypeSeparator, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the maximum width, in characters, of auto-completion and user lists.
	 */
	int32_t Editor::AutoCMaxWidth()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetMaxWidth, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the maximum width, in characters, of auto-completion and user lists.
	 * Set to 0 to autosize to fit longest item, which is the default.
	 */
	void Editor::AutoCMaxWidth(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetMaxWidth, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the maximum height, in rows, of auto-completion and user lists.
	 */
	int32_t Editor::AutoCMaxHeight()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetMaxHeight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the maximum height, in rows, of auto-completion and user lists.
	 * The default is 5 rows.
	 */
	void Editor::AutoCMaxHeight(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetMaxHeight, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the style number used for auto-completion and user lists fonts.
	 */
	int32_t Editor::AutoCStyle()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetStyle, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the style number used for auto-completion and user lists fonts.
	 */
	void Editor::AutoCStyle(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetStyle, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the scale factor in percent for auto-completion list images.
	 */
	int32_t Editor::AutoCImageScale()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetImageScale, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the scale factor in percent for auto-completion list images.
	 */
	void Editor::AutoCImageScale(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetImageScale, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve indentation size.
	 */
	int32_t Editor::Indent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetIndent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the number of spaces used for one level of indentation.
	 */
	void Editor::Indent(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIndent, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether tabs will be used in indentation.
	 */
	bool Editor::UseTabs()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetUseTabs, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Indentation will only use space characters if useTabs is false, otherwise
	 * it will use a combination of tabs and spaces.
	 */
	void Editor::UseTabs(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUseTabs, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is the horizontal scroll bar visible?
	 */
	bool Editor::HScrollBar()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetHScrollBar, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Show or hide the horizontal scroll bar.
	 */
	void Editor::HScrollBar(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetHScrollBar, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Are the indentation guides visible?
	 */
	WinUIEditor::IndentView Editor::IndentationGuides()
	{
		return static_cast<WinUIEditor::IndentView>(_editor.get()->PublicWndProc(Scintilla::Message::GetIndentationGuides, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Show or hide indentation guides.
	 */
	void Editor::IndentationGuides(WinUIEditor::IndentView const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIndentationGuides, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the highlighted indentation guide column.
	 */
	int64_t Editor::HighlightGuide()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetHighlightGuide, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the highlighted indentation guide column.
	 * 0 = no highlighted guide.
	 */
	void Editor::HighlightGuide(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetHighlightGuide, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the code page used to interpret the bytes of the document as characters.
	 */
	int32_t Editor::CodePage()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCodePage, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the code page used to interpret the bytes of the document as characters.
	 * The SC_CP_UTF8 value can be used to enter Unicode mode.
	 */
	void Editor::CodePage(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCodePage, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the foreground colour of the caret.
	 */
	int32_t Editor::CaretFore()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretFore, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the foreground colour of the caret.
	 */
	void Editor::CaretFore(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretFore, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * In read-only mode?
	 */
	bool Editor::ReadOnly()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetReadOnly, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set to read only or read write.
	 */
	void Editor::ReadOnly(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetReadOnly, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the position at the start of the selection.
	 */
	int64_t Editor::SelectionStart()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the position that starts the selection - this becomes the anchor.
	 */
	void Editor::SelectionStart(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionStart, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the position at the end of the selection.
	 */
	int64_t Editor::SelectionEnd()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the position that ends the selection - this becomes the caret.
	 */
	void Editor::SelectionEnd(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionEnd, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the print magnification.
	 */
	int32_t Editor::PrintMagnification()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPrintMagnification, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the print magnification added to the point size of each style for printing.
	 */
	void Editor::PrintMagnification(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPrintMagnification, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the print colour mode.
	 */
	WinUIEditor::PrintOption Editor::PrintColourMode()
	{
		return static_cast<WinUIEditor::PrintOption>(_editor.get()->PublicWndProc(Scintilla::Message::GetPrintColourMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Modify colours when printing for clearer printed text.
	 */
	void Editor::PrintColourMode(WinUIEditor::PrintOption const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPrintColourMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Report change history status.
	 */
	WinUIEditor::ChangeHistoryOption Editor::ChangeHistory()
	{
		return static_cast<WinUIEditor::ChangeHistoryOption>(_editor.get()->PublicWndProc(Scintilla::Message::GetChangeHistory, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Enable or disable change history.
	 */
	void Editor::ChangeHistory(WinUIEditor::ChangeHistoryOption const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetChangeHistory, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Report undo selection history status.
	 */
	WinUIEditor::UndoSelectionHistoryOption Editor::UndoSelectionHistory()
	{
		return static_cast<WinUIEditor::UndoSelectionHistoryOption>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoSelectionHistory, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Enable or disable undo selection history.
	 */
	void Editor::UndoSelectionHistory(WinUIEditor::UndoSelectionHistoryOption const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetUndoSelectionHistory, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the display line at the top of the display.
	 */
	int64_t Editor::FirstVisibleLine()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetFirstVisibleLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Scroll so that a display line is at the top of the display.
	 */
	void Editor::FirstVisibleLine(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFirstVisibleLine, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the number of lines in the document. There is always at least one.
	 */
	int64_t Editor::LineCount()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineCount, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the size in pixels of the left margin.
	 */
	int32_t Editor::MarginLeft()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the size in pixels of the left margin.
	 */
	void Editor::MarginLeft(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(value));
	}

	/**
	 * Returns the size in pixels of the right margin.
	 */
	int32_t Editor::MarginRight()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the size in pixels of the right margin.
	 */
	void Editor::MarginRight(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(value));
	}

	/**
	 * Is the document different from when it was last saved?
	 */
	bool Editor::Modify()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetModify, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	bool Editor::SelectionHidden()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionHidden, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the number of characters in the document.
	 */
	int64_t Editor::TextLength()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTextLength, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve a pointer to a function that processes messages for this Scintilla.
	 */
	uint64_t Editor::DirectFunction()
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetDirectFunction, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve a pointer to a function that processes messages for this Scintilla and returns status.
	 */
	uint64_t Editor::DirectStatusFunction()
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetDirectStatusFunction, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve a pointer value to use as the first argument when calling
	 * the function returned by GetDirectFunction.
	 */
	uint64_t Editor::DirectPointer()
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetDirectPointer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns true if overtype mode is active otherwise false is returned.
	 */
	bool Editor::Overtype()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetOvertype, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set to overtype (true) or insert mode.
	 */
	void Editor::Overtype(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetOvertype, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the width of the insert mode caret.
	 */
	int32_t Editor::CaretWidth()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretWidth, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the width of the insert mode caret.
	 */
	void Editor::CaretWidth(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretWidth, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the position that starts the target.
	 */
	int64_t Editor::TargetStart()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTargetStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the position that starts the target which is used for updating the
	 * document without affecting the scroll position.
	 */
	void Editor::TargetStart(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTargetStart, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the virtual space of the target start
	 */
	int64_t Editor::TargetStartVirtualSpace()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTargetStartVirtualSpace, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the virtual space of the target start
	 */
	void Editor::TargetStartVirtualSpace(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTargetStartVirtualSpace, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the position that ends the target.
	 */
	int64_t Editor::TargetEnd()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTargetEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the position that ends the target which is used for updating the
	 * document without affecting the scroll position.
	 */
	void Editor::TargetEnd(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTargetEnd, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the virtual space of the target end
	 */
	int64_t Editor::TargetEndVirtualSpace()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTargetEndVirtualSpace, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the virtual space of the target end
	 */
	void Editor::TargetEndVirtualSpace(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTargetEndVirtualSpace, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the search flags used by SearchInTarget.
	 */
	WinUIEditor::FindOption Editor::SearchFlags()
	{
		return static_cast<WinUIEditor::FindOption>(_editor.get()->PublicWndProc(Scintilla::Message::GetSearchFlags, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the search flags used by SearchInTarget.
	 */
	void Editor::SearchFlags(WinUIEditor::FindOption const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSearchFlags, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Are all lines visible?
	 */
	bool Editor::AllLinesVisible()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetAllLinesVisible, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the style of fold display text.
	 */
	WinUIEditor::FoldDisplayTextStyle Editor::FoldDisplayTextStyle()
	{
		return static_cast<WinUIEditor::FoldDisplayTextStyle>(_editor.get()->PublicWndProc(Scintilla::Message::FoldDisplayTextGetStyle, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the style of fold display text.
	 */
	void Editor::FoldDisplayTextStyle(WinUIEditor::FoldDisplayTextStyle const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FoldDisplayTextSetStyle, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get automatic folding behaviours.
	 */
	WinUIEditor::AutomaticFold Editor::AutomaticFold()
	{
		return static_cast<WinUIEditor::AutomaticFold>(_editor.get()->PublicWndProc(Scintilla::Message::GetAutomaticFold, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set automatic folding behaviours.
	 */
	void Editor::AutomaticFold(WinUIEditor::AutomaticFold const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAutomaticFold, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Does a tab pressed when caret is within indentation indent?
	 */
	bool Editor::TabIndents()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetTabIndents, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets whether a tab pressed when caret is within indentation indents.
	 */
	void Editor::TabIndents(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTabIndents, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Does a backspace pressed when caret is within indentation unindent?
	 */
	bool Editor::BackSpaceUnIndents()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetBackSpaceUnIndents, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets whether a backspace pressed when caret is within indentation unindents.
	 */
	void Editor::BackSpaceUnIndents(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetBackSpaceUnIndents, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the time the mouse must sit still to generate a mouse dwell event.
	 */
	int32_t Editor::MouseDwellTime()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMouseDwellTime, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the time the mouse must sit still to generate a mouse dwell event.
	 */
	void Editor::MouseDwellTime(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMouseDwellTime, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the limits to idle styling.
	 */
	WinUIEditor::IdleStyling Editor::IdleStyling()
	{
		return static_cast<WinUIEditor::IdleStyling>(_editor.get()->PublicWndProc(Scintilla::Message::GetIdleStyling, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets limits to idle styling.
	 */
	void Editor::IdleStyling(WinUIEditor::IdleStyling const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIdleStyling, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether text is word wrapped.
	 */
	WinUIEditor::Wrap Editor::WrapMode()
	{
		return static_cast<WinUIEditor::Wrap>(_editor.get()->PublicWndProc(Scintilla::Message::GetWrapMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets whether text is word wrapped.
	 */
	void Editor::WrapMode(WinUIEditor::Wrap const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWrapMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrive the display mode of visual flags for wrapped lines.
	 */
	WinUIEditor::WrapVisualFlag Editor::WrapVisualFlags()
	{
		return static_cast<WinUIEditor::WrapVisualFlag>(_editor.get()->PublicWndProc(Scintilla::Message::GetWrapVisualFlags, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the display mode of visual flags for wrapped lines.
	 */
	void Editor::WrapVisualFlags(WinUIEditor::WrapVisualFlag const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWrapVisualFlags, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrive the location of visual flags for wrapped lines.
	 */
	WinUIEditor::WrapVisualLocation Editor::WrapVisualFlagsLocation()
	{
		return static_cast<WinUIEditor::WrapVisualLocation>(_editor.get()->PublicWndProc(Scintilla::Message::GetWrapVisualFlagsLocation, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the location of visual flags for wrapped lines.
	 */
	void Editor::WrapVisualFlagsLocation(WinUIEditor::WrapVisualLocation const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWrapVisualFlagsLocation, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrive the start indent for wrapped lines.
	 */
	int32_t Editor::WrapStartIndent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetWrapStartIndent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the start indent for wrapped lines.
	 */
	void Editor::WrapStartIndent(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWrapStartIndent, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve how wrapped sublines are placed. Default is fixed.
	 */
	WinUIEditor::WrapIndentMode Editor::WrapIndentMode()
	{
		return static_cast<WinUIEditor::WrapIndentMode>(_editor.get()->PublicWndProc(Scintilla::Message::GetWrapIndentMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets how wrapped sublines are placed. Default is fixed.
	 */
	void Editor::WrapIndentMode(WinUIEditor::WrapIndentMode const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWrapIndentMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the degree of caching of layout information.
	 */
	WinUIEditor::LineCache Editor::LayoutCache()
	{
		return static_cast<WinUIEditor::LineCache>(_editor.get()->PublicWndProc(Scintilla::Message::GetLayoutCache, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the degree of caching of layout information.
	 */
	void Editor::LayoutCache(WinUIEditor::LineCache const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetLayoutCache, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the document width assumed for scrolling.
	 */
	int32_t Editor::ScrollWidth()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetScrollWidth, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the document width assumed for scrolling.
	 */
	void Editor::ScrollWidth(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetScrollWidth, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether the scroll width tracks wide lines.
	 */
	bool Editor::ScrollWidthTracking()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetScrollWidthTracking, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets whether the maximum width line displayed is used to set scroll width.
	 */
	void Editor::ScrollWidthTracking(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetScrollWidthTracking, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve whether the maximum scroll position has the last
	 * line at the bottom of the view.
	 */
	bool Editor::EndAtLastLine()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetEndAtLastLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the scroll range so that maximum scroll position has
	 * the last line at the bottom of the view (default).
	 * Setting this to false allows scrolling one page below the last line.
	 */
	void Editor::EndAtLastLine(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetEndAtLastLine, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is the vertical scroll bar visible?
	 */
	bool Editor::VScrollBar()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetVScrollBar, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Show or hide the vertical scroll bar.
	 */
	void Editor::VScrollBar(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetVScrollBar, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * How many phases is drawing done in?
	 */
	WinUIEditor::PhasesDraw Editor::PhasesDraw()
	{
		return static_cast<WinUIEditor::PhasesDraw>(_editor.get()->PublicWndProc(Scintilla::Message::GetPhasesDraw, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * In one phase draw, text is drawn in a series of rectangular blocks with no overlap.
	 * In two phase draw, text is drawn in a series of lines allowing runs to overlap horizontally.
	 * In multiple phase draw, each element is drawn over the whole drawing area, allowing text
	 * to overlap from one line to the next.
	 */
	void Editor::PhasesDraw(WinUIEditor::PhasesDraw const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPhasesDraw, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the quality level for text.
	 */
	WinUIEditor::FontQuality Editor::FontQuality()
	{
		return static_cast<WinUIEditor::FontQuality>(_editor.get()->PublicWndProc(Scintilla::Message::GetFontQuality, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Choose the quality level for text from the FontQuality enumeration.
	 */
	void Editor::FontQuality(WinUIEditor::FontQuality const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFontQuality, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the effect of pasting when there are multiple selections.
	 */
	WinUIEditor::MultiPaste Editor::MultiPaste()
	{
		return static_cast<WinUIEditor::MultiPaste>(_editor.get()->PublicWndProc(Scintilla::Message::GetMultiPaste, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the effect of pasting when there are multiple selections.
	 */
	void Editor::MultiPaste(WinUIEditor::MultiPaste const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMultiPaste, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Report accessibility status.
	 */
	WinUIEditor::Accessibility Editor::Accessibility()
	{
		return static_cast<WinUIEditor::Accessibility>(_editor.get()->PublicWndProc(Scintilla::Message::GetAccessibility, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Enable or disable accessibility.
	 */
	void Editor::Accessibility(WinUIEditor::Accessibility const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAccessibility, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Are the end of line characters visible?
	 */
	bool Editor::ViewEOL()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetViewEOL, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Make the end of line characters visible or invisible.
	 */
	void Editor::ViewEOL(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetViewEOL, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve a pointer to the document object.
	 */
	uint64_t Editor::DocPointer()
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetDocPointer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the document object used.
	 */
	void Editor::DocPointer(uint64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetDocPointer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(value));
	}

	/**
	 * Retrieve the column number which text should be kept within.
	 */
	int64_t Editor::EdgeColumn()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetEdgeColumn, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the column number of the edge.
	 * If text goes past the edge then it is highlighted.
	 */
	void Editor::EdgeColumn(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetEdgeColumn, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the edge highlight mode.
	 */
	WinUIEditor::EdgeVisualStyle Editor::EdgeMode()
	{
		return static_cast<WinUIEditor::EdgeVisualStyle>(_editor.get()->PublicWndProc(Scintilla::Message::GetEdgeMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * The edge may be displayed by a line (EDGE_LINE/EDGE_MULTILINE) or by highlighting text that
	 * goes beyond it (EDGE_BACKGROUND) or not displayed at all (EDGE_NONE).
	 */
	void Editor::EdgeMode(WinUIEditor::EdgeVisualStyle const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetEdgeMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the colour used in edge indication.
	 */
	int32_t Editor::EdgeColour()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetEdgeColour, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the colour used in edge indication.
	 */
	void Editor::EdgeColour(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetEdgeColour, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieves the number of lines completely visible.
	 */
	int64_t Editor::LinesOnScreen()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::LinesOnScreen, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Is the selection rectangular? The alternative is the more common stream selection.
	 */
	bool Editor::SelectionIsRectangle()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::SelectionIsRectangle, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the zoom level.
	 */
	int32_t Editor::Zoom()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetZoom, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the zoom level. This number of points is added to the size of all fonts.
	 * It may be positive to magnify or negative to reduce.
	 */
	void Editor::Zoom(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetZoom, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get which document options are set.
	 */
	WinUIEditor::DocumentOption Editor::DocumentOptions()
	{
		return static_cast<WinUIEditor::DocumentOption>(_editor.get()->PublicWndProc(Scintilla::Message::GetDocumentOptions, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get which document modification events are sent to the container.
	 */
	WinUIEditor::ModificationFlags Editor::ModEventMask()
	{
		return static_cast<WinUIEditor::ModificationFlags>(_editor.get()->PublicWndProc(Scintilla::Message::GetModEventMask, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set which document modification events are sent to the container.
	 */
	void Editor::ModEventMask(WinUIEditor::ModificationFlags const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetModEventMask, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get whether command events are sent to the container.
	 */
	bool Editor::CommandEvents()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetCommandEvents, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether command events are sent to the container.
	 */
	void Editor::CommandEvents(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCommandEvents, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get internal focus flag.
	 */
	bool Editor::Focus()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetFocus, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change internal focus flag.
	 */
	void Editor::Focus(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFocus, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get error status.
	 */
	WinUIEditor::Status Editor::Status()
	{
		return static_cast<WinUIEditor::Status>(_editor.get()->PublicWndProc(Scintilla::Message::GetStatus, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change error status - 0 = OK.
	 */
	void Editor::Status(WinUIEditor::Status const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetStatus, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get whether mouse gets captured.
	 */
	bool Editor::MouseDownCaptures()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetMouseDownCaptures, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether the mouse is captured when its button is pressed.
	 */
	void Editor::MouseDownCaptures(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMouseDownCaptures, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get whether mouse wheel can be active outside the window.
	 */
	bool Editor::MouseWheelCaptures()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetMouseWheelCaptures, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether the mouse wheel can be active outside the window.
	 */
	void Editor::MouseWheelCaptures(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMouseWheelCaptures, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get cursor type.
	 */
	WinUIEditor::CursorShape Editor::Cursor()
	{
		return static_cast<WinUIEditor::CursorShape>(_editor.get()->PublicWndProc(Scintilla::Message::GetCursor, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the cursor to one of the SC_CURSOR* values.
	 */
	void Editor::Cursor(WinUIEditor::CursorShape const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCursor, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the way control characters are displayed.
	 */
	int32_t Editor::ControlCharSymbol()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetControlCharSymbol, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the way control characters are displayed:
	 * If symbol is < 32, keep the drawn way, else, use the given character.
	 */
	void Editor::ControlCharSymbol(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetControlCharSymbol, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the xOffset (ie, horizontal scroll position).
	 */
	int32_t Editor::XOffset()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetXOffset, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the xOffset (ie, horizontal scroll position).
	 */
	void Editor::XOffset(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetXOffset, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is printing line wrapped?
	 */
	WinUIEditor::Wrap Editor::PrintWrapMode()
	{
		return static_cast<WinUIEditor::Wrap>(_editor.get()->PublicWndProc(Scintilla::Message::GetPrintWrapMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set printing to line wrapped (SC_WRAP_WORD) or not line wrapped (SC_WRAP_NONE).
	 */
	void Editor::PrintWrapMode(WinUIEditor::Wrap const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPrintWrapMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get whether underlining for active hotspots.
	 */
	bool Editor::HotspotActiveUnderline()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetHotspotActiveUnderline, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Enable / Disable underlining active hotspots.
	 */
	void Editor::HotspotActiveUnderline(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetHotspotActiveUnderline, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the HotspotSingleLine property
	 */
	bool Editor::HotspotSingleLine()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetHotspotSingleLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Limit hotspots to single line so hotspots on two lines don't merge.
	 */
	void Editor::HotspotSingleLine(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetHotspotSingleLine, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the mode of the current selection.
	 */
	WinUIEditor::SelectionMode Editor::SelectionMode()
	{
		return static_cast<WinUIEditor::SelectionMode>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionMode, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the selection mode to stream (SC_SEL_STREAM) or rectangular (SC_SEL_RECTANGLE/SC_SEL_THIN) or
	 * by lines (SC_SEL_LINES).
	 */
	void Editor::SelectionMode(WinUIEditor::SelectionMode const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionMode, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get whether or not regular caret moves will extend or reduce the selection.
	 */
	bool Editor::MoveExtendsSelection()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetMoveExtendsSelection, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether or not regular caret moves will extend or reduce the selection.
	 */
	void Editor::MoveExtendsSelection(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMoveExtendsSelection, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get currently selected item position in the auto-completion list
	 */
	int32_t Editor::AutoCCurrent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetCurrent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get auto-completion case insensitive behaviour.
	 */
	WinUIEditor::CaseInsensitiveBehaviour Editor::AutoCCaseInsensitiveBehaviour()
	{
		return static_cast<WinUIEditor::CaseInsensitiveBehaviour>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetCaseInsensitiveBehaviour, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set auto-completion case insensitive behaviour to either prefer case-sensitive matches or have no preference.
	 */
	void Editor::AutoCCaseInsensitiveBehaviour(WinUIEditor::CaseInsensitiveBehaviour const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetCaseInsensitiveBehaviour, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the effect of autocompleting when there are multiple selections.
	 */
	WinUIEditor::MultiAutoComplete Editor::AutoCMulti()
	{
		return static_cast<WinUIEditor::MultiAutoComplete>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetMulti, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Change the effect of autocompleting when there are multiple selections.
	 */
	void Editor::AutoCMulti(WinUIEditor::MultiAutoComplete const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetMulti, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the way autocompletion lists are ordered.
	 */
	WinUIEditor::Ordering Editor::AutoCOrder()
	{
		return static_cast<WinUIEditor::Ordering>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetOrder, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the way autocompletion lists are ordered.
	 */
	void Editor::AutoCOrder(WinUIEditor::Ordering const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetOrder, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Can the caret preferred x position only be changed by explicit movement commands?
	 */
	WinUIEditor::CaretSticky Editor::CaretSticky()
	{
		return static_cast<WinUIEditor::CaretSticky>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretSticky, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Stop the caret preferred x position changing when the user types.
	 */
	void Editor::CaretSticky(WinUIEditor::CaretSticky const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretSticky, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get convert-on-paste setting
	 */
	bool Editor::PasteConvertEndings()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetPasteConvertEndings, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Enable/Disable convert-on-paste for line endings
	 */
	void Editor::PasteConvertEndings(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPasteConvertEndings, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the background alpha of the caret line.
	 */
	WinUIEditor::Alpha Editor::CaretLineBackAlpha()
	{
		return static_cast<WinUIEditor::Alpha>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineBackAlpha, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set background alpha of the caret line.
	 */
	void Editor::CaretLineBackAlpha(WinUIEditor::Alpha const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineBackAlpha, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the current style of the caret.
	 */
	WinUIEditor::CaretStyle Editor::CaretStyle()
	{
		return static_cast<WinUIEditor::CaretStyle>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretStyle, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the style of the caret to be drawn.
	 */
	void Editor::CaretStyle(WinUIEditor::CaretStyle const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretStyle, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the current indicator
	 */
	int32_t Editor::IndicatorCurrent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetIndicatorCurrent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the indicator used for IndicatorFillRange and IndicatorClearRange
	 */
	void Editor::IndicatorCurrent(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIndicatorCurrent, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the current indicator value
	 */
	int32_t Editor::IndicatorValue()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetIndicatorValue, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the value used for IndicatorFillRange
	 */
	void Editor::IndicatorValue(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIndicatorValue, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * How many entries are allocated to the position cache?
	 */
	int32_t Editor::PositionCache()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPositionCache, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set number of entries in position cache
	 */
	void Editor::PositionCache(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPositionCache, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get maximum number of threads used for layout
	 */
	int32_t Editor::LayoutThreads()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLayoutThreads, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set maximum number of threads used for layout
	 */
	void Editor::LayoutThreads(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetLayoutThreads, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Compact the document buffer and return a read-only pointer to the
	 * characters in the document.
	 */
	uint64_t Editor::CharacterPointer()
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCharacterPointer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Return a position which, to avoid performance costs, should not be within
	 * the range of a call to GetRangePointer.
	 */
	int64_t Editor::GapPosition()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetGapPosition, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get extra ascent for each line
	 */
	int32_t Editor::ExtraAscent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetExtraAscent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set extra ascent for each line
	 */
	void Editor::ExtraAscent(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetExtraAscent, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get extra descent for each line
	 */
	int32_t Editor::ExtraDescent()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetExtraDescent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set extra descent for each line
	 */
	void Editor::ExtraDescent(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetExtraDescent, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the start of the range of style numbers used for margin text
	 */
	int32_t Editor::MarginStyleOffset()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarginGetStyleOffset, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the start of the range of style numbers used for margin text
	 */
	void Editor::MarginStyleOffset(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarginSetStyleOffset, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the margin options.
	 */
	WinUIEditor::MarginOption Editor::MarginOptions()
	{
		return static_cast<WinUIEditor::MarginOption>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginOptions, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the margin options.
	 */
	void Editor::MarginOptions(WinUIEditor::MarginOption const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginOptions, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the visibility for the annotations for a view
	 */
	WinUIEditor::AnnotationVisible Editor::AnnotationVisible()
	{
		return static_cast<WinUIEditor::AnnotationVisible>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetVisible, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the visibility for the annotations for a view
	 */
	void Editor::AnnotationVisible(WinUIEditor::AnnotationVisible const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetVisible, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the start of the range of style numbers used for annotations
	 */
	int32_t Editor::AnnotationStyleOffset()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetStyleOffset, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the start of the range of style numbers used for annotations
	 */
	void Editor::AnnotationStyleOffset(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetStyleOffset, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Whether switching to rectangular mode while selecting with the mouse is allowed.
	 */
	bool Editor::MouseSelectionRectangularSwitch()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetMouseSelectionRectangularSwitch, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether switching to rectangular mode while selecting with the mouse is allowed.
	 */
	void Editor::MouseSelectionRectangularSwitch(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMouseSelectionRectangularSwitch, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Whether multiple selections can be made
	 */
	bool Editor::MultipleSelection()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetMultipleSelection, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether multiple selections can be made
	 */
	void Editor::MultipleSelection(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMultipleSelection, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Whether typing can be performed into multiple selections
	 */
	bool Editor::AdditionalSelectionTyping()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetAdditionalSelectionTyping, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether typing can be performed into multiple selections
	 */
	void Editor::AdditionalSelectionTyping(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalSelectionTyping, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Whether additional carets will blink
	 */
	bool Editor::AdditionalCaretsBlink()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetAdditionalCaretsBlink, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether additional carets will blink
	 */
	void Editor::AdditionalCaretsBlink(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalCaretsBlink, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Whether additional carets are visible
	 */
	bool Editor::AdditionalCaretsVisible()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetAdditionalCaretsVisible, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set whether additional carets are visible
	 */
	void Editor::AdditionalCaretsVisible(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalCaretsVisible, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * How many selections are there?
	 */
	int32_t Editor::Selections()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelections, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Is every selected range empty?
	 */
	bool Editor::SelectionEmpty()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionEmpty, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Which selection is the main selection
	 */
	int32_t Editor::MainSelection()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMainSelection, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the main selection
	 */
	void Editor::MainSelection(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMainSelection, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Return the caret position of the rectangular selection.
	 */
	int64_t Editor::RectangularSelectionCaret()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRectangularSelectionCaret, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the caret position of the rectangular selection.
	 */
	void Editor::RectangularSelectionCaret(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRectangularSelectionCaret, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Return the anchor position of the rectangular selection.
	 */
	int64_t Editor::RectangularSelectionAnchor()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRectangularSelectionAnchor, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the anchor position of the rectangular selection.
	 */
	void Editor::RectangularSelectionAnchor(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRectangularSelectionAnchor, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Return the virtual space of the caret of the rectangular selection.
	 */
	int64_t Editor::RectangularSelectionCaretVirtualSpace()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRectangularSelectionCaretVirtualSpace, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the virtual space of the caret of the rectangular selection.
	 */
	void Editor::RectangularSelectionCaretVirtualSpace(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRectangularSelectionCaretVirtualSpace, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Return the virtual space of the anchor of the rectangular selection.
	 */
	int64_t Editor::RectangularSelectionAnchorVirtualSpace()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRectangularSelectionAnchorVirtualSpace, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the virtual space of the anchor of the rectangular selection.
	 */
	void Editor::RectangularSelectionAnchorVirtualSpace(int64_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRectangularSelectionAnchorVirtualSpace, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Return options for virtual space behaviour.
	 */
	WinUIEditor::VirtualSpace Editor::VirtualSpaceOptions()
	{
		return static_cast<WinUIEditor::VirtualSpace>(_editor.get()->PublicWndProc(Scintilla::Message::GetVirtualSpaceOptions, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set options for virtual space behaviour.
	 */
	void Editor::VirtualSpaceOptions(WinUIEditor::VirtualSpace const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetVirtualSpaceOptions, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the modifier key used for rectangular selection.
	 */
	int32_t Editor::RectangularSelectionModifier()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRectangularSelectionModifier, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	void Editor::RectangularSelectionModifier(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRectangularSelectionModifier, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the alpha of the selection.
	 */
	WinUIEditor::Alpha Editor::AdditionalSelAlpha()
	{
		return static_cast<WinUIEditor::Alpha>(_editor.get()->PublicWndProc(Scintilla::Message::GetAdditionalSelAlpha, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the alpha of the selection.
	 */
	void Editor::AdditionalSelAlpha(WinUIEditor::Alpha const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalSelAlpha, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the foreground colour of additional carets.
	 */
	int32_t Editor::AdditionalCaretFore()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetAdditionalCaretFore, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the foreground colour of additional carets.
	 */
	void Editor::AdditionalCaretFore(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalCaretFore, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the identifier.
	 */
	int32_t Editor::Identifier()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetIdentifier, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the identifier reported as idFrom in notification messages.
	 */
	void Editor::Identifier(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIdentifier, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the tech.
	 */
	WinUIEditor::Technology Editor::Technology()
	{
		return static_cast<WinUIEditor::Technology>(_editor.get()->PublicWndProc(Scintilla::Message::GetTechnology, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the technology used.
	 */
	void Editor::Technology(WinUIEditor::Technology const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTechnology, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is the caret line always visible?
	 */
	bool Editor::CaretLineVisibleAlways()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetCaretLineVisibleAlways, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Sets the caret line to always visible.
	 */
	void Editor::CaretLineVisibleAlways(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCaretLineVisibleAlways, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the line end types currently allowed.
	 */
	WinUIEditor::LineEndType Editor::LineEndTypesAllowed()
	{
		return static_cast<WinUIEditor::LineEndType>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineEndTypesAllowed, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the line end types that the application wants to use. May not be used if incompatible with lexer or encoding.
	 */
	void Editor::LineEndTypesAllowed(WinUIEditor::LineEndType const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetLineEndTypesAllowed, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the line end types currently recognised. May be a subset of the allowed types due to lexer limitation.
	 */
	WinUIEditor::LineEndType Editor::LineEndTypesActive()
	{
		return static_cast<WinUIEditor::LineEndType>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineEndTypesActive, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the visibility for the end of line annotations for a view
	 */
	WinUIEditor::EOLAnnotationVisible Editor::EOLAnnotationVisible()
	{
		return static_cast<WinUIEditor::EOLAnnotationVisible>(_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationGetVisible, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set the visibility for the end of line annotations for a view
	 */
	void Editor::EOLAnnotationVisible(WinUIEditor::EOLAnnotationVisible const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationSetVisible, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get the start of the range of style numbers used for end of line annotations
	 */
	int32_t Editor::EOLAnnotationStyleOffset()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationGetStyleOffset, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the start of the range of style numbers used for end of line annotations
	 */
	void Editor::EOLAnnotationStyleOffset(int32_t value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationSetStyleOffset, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve line character index state.
	 */
	WinUIEditor::LineCharacterIndexType Editor::LineCharacterIndex()
	{
		return static_cast<WinUIEditor::LineCharacterIndexType>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineCharacterIndex, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get whether drag-and-drop is enabled or disabled
	 */
	bool Editor::DragDropEnabled()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetDragDropEnabled, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Enable or disable drag-and-drop
	 */
	void Editor::DragDropEnabled(bool value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetDragDropEnabled, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the lexing language of the document.
	 */
	int32_t Editor::Lexer()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLexer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Bit set of LineEndType enumertion for which line ends beyond the standard
	 * LF, CR, and CRLF are supported by the lexer.
	 */
	WinUIEditor::LineEndType Editor::LineEndTypesSupported()
	{
		return static_cast<WinUIEditor::LineEndType>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineEndTypesSupported, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Where styles are duplicated by a feature such as active/inactive code
	 * return the distance between the two types.
	 */
	int32_t Editor::DistanceToSecondaryStyles()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::DistanceToSecondaryStyles, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the number of named styles for the lexer.
	 */
	int32_t Editor::NamedStyles()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetNamedStyles, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve bidirectional text display state.
	 */
	WinUIEditor::Bidirectional Editor::Bidirectional()
	{
		return static_cast<WinUIEditor::Bidirectional>(_editor.get()->PublicWndProc(Scintilla::Message::GetBidirectional, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Set bidirectional text display state.
	 */
	void Editor::Bidirectional(WinUIEditor::Bidirectional const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetBidirectional, static_cast<Scintilla::uptr_t>(value), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Add text to the document at current position.
	 */
	void Editor::AddTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AddText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::AddText(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::AddText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Add array of cells to document.
	 */
	void Editor::AddStyledText(int64_t length, array_view<uint8_t const> c)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AddStyledText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(c.data()));
	}

	/**
	 * Insert string at a position.
	 */
	void Editor::InsertTextFromBuffer(int64_t pos, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::InsertText, static_cast<Scintilla::uptr_t>(pos), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::InsertText(int64_t pos, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::InsertText, static_cast<Scintilla::uptr_t>(pos), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Change the text that is being inserted in response to SC_MOD_INSERTCHECK
	 */
	void Editor::ChangeInsertionFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ChangeInsertion, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::ChangeInsertion(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::ChangeInsertion, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Delete all text in the document.
	 */
	void Editor::ClearAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete a range of text in the document.
	 */
	void Editor::DeleteRange(int64_t start, int64_t lengthDelete)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DeleteRange, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(lengthDelete));
	}

	/**
	 * Set all style bytes to 0, remove all folding information.
	 */
	void Editor::ClearDocumentStyle()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearDocumentStyle, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Redoes the next action on the undo history.
	 */
	void Editor::Redo()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Redo, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Select all the text in the document.
	 */
	void Editor::SelectAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SelectAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Remember the current position in the undo history as the position
	 * at which the document was saved.
	 */
	void Editor::SetSavePoint()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSavePoint, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve a buffer of cells.
	 * Returns the number of bytes in the buffer not including terminating NULs.
	 */
	int64_t Editor::GetStyledText(uint64_t tr)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetStyledText, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(tr)));
	}

	/**
	 * Retrieve a buffer of cells that can be past 2GB.
	 * Returns the number of bytes in the buffer not including terminating NULs.
	 */
	int64_t Editor::GetStyledTextFull(uint64_t tr)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetStyledTextFull, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(tr)));
	}

	/**
	 * Are there any redoable actions in the undo history?
	 */
	bool Editor::CanRedo()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::CanRedo, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the line number at which a particular marker is located.
	 */
	int64_t Editor::MarkerLineFromHandle(int32_t markerHandle)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerLineFromHandle, static_cast<Scintilla::uptr_t>(markerHandle), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Delete a marker.
	 */
	void Editor::MarkerDeleteHandle(int32_t markerHandle)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDeleteHandle, static_cast<Scintilla::uptr_t>(markerHandle), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve marker handles of a line
	 */
	int32_t Editor::MarkerHandleFromLine(int64_t line, int32_t which)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerHandleFromLine, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(which)));
	}

	/**
	 * Retrieve marker number of a marker handle
	 */
	int32_t Editor::MarkerNumberFromLine(int64_t line, int32_t which)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerNumberFromLine, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(which)));
	}

	/**
	 * Find the position from a point within the window.
	 */
	int64_t Editor::PositionFromPoint(int32_t x, int32_t y)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionFromPoint, static_cast<Scintilla::uptr_t>(x), static_cast<Scintilla::sptr_t>(y)));
	}

	/**
	 * Find the position from a point within the window but return
	 * INVALID_POSITION if not close to text.
	 */
	int64_t Editor::PositionFromPointClose(int32_t x, int32_t y)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionFromPointClose, static_cast<Scintilla::uptr_t>(x), static_cast<Scintilla::sptr_t>(y)));
	}

	/**
	 * Set caret to start of a line and ensure it is visible.
	 */
	void Editor::GotoLine(int64_t line)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::GotoLine, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set caret to a position and ensure it is visible.
	 */
	void Editor::GotoPos(int64_t caret)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::GotoPos, static_cast<Scintilla::uptr_t>(caret), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the text of the line containing the caret.
	 * Returns the index of the caret on the line.
	 * Result is NUL-terminated.
	 */
	int64_t Editor::GetCurLineWriteBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCurLine, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetCurLine(int64_t length)
	{
		if (length <= 0) return hstring{};
		const auto wParam{ static_cast<Scintilla::uptr_t>(length) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCurLine, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ std::min(sourceLength, static_cast<size_t>(length)) };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetCurLine, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Convert all line endings in the document to one mode.
	 */
	void Editor::ConvertEOLs(WinUIEditor::EndOfLine const &eolMode)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ConvertEOLs, static_cast<Scintilla::uptr_t>(eolMode), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the current styling position to start.
	 * The unused parameter is no longer used and should be set to 0.
	 */
	void Editor::StartStyling(int64_t start, int32_t unused)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StartStyling, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(unused));
	}

	/**
	 * Change style from current styling position for length characters to a style
	 * and move the current styling position to after this newly styled segment.
	 */
	void Editor::SetStyling(int64_t length, int32_t style)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetStyling, static_cast<Scintilla::uptr_t>(length), static_cast<Scintilla::sptr_t>(style));
	}

	/**
	 * Clear explicit tabstops on a line.
	 */
	void Editor::ClearTabStops(int64_t line)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearTabStops, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Add an explicit tab stop for a line.
	 */
	void Editor::AddTabStop(int64_t line, int32_t x)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AddTabStop, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(x));
	}

	/**
	 * Find the next explicit tab stop position on a line after a position.
	 */
	int32_t Editor::GetNextTabStop(int64_t line, int32_t x)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetNextTabStop, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(x)));
	}

	/**
	 * Set the symbol used for a particular marker number.
	 */
	void Editor::MarkerDefine(int32_t markerNumber, WinUIEditor::MarkerSymbol const &markerSymbol)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDefine, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(markerSymbol));
	}

	/**
	 * Enable/disable highlight for current folding block (smallest one that contains the caret)
	 */
	void Editor::MarkerEnableHighlight(bool enabled)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerEnableHighlight, static_cast<Scintilla::uptr_t>(enabled), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Add a marker to a line, returning an ID which can be used to find or delete the marker.
	 */
	int32_t Editor::MarkerAdd(int64_t line, int32_t markerNumber)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerAdd, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(markerNumber)));
	}

	/**
	 * Delete a marker from a line.
	 */
	void Editor::MarkerDelete(int64_t line, int32_t markerNumber)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDelete, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(markerNumber));
	}

	/**
	 * Delete all markers with a particular number from all lines.
	 */
	void Editor::MarkerDeleteAll(int32_t markerNumber)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDeleteAll, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get a bit mask of all the markers set on a line.
	 */
	int32_t Editor::MarkerGet(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerGet, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Find the next line at or after lineStart that includes a marker in mask.
	 * Return -1 when no more lines.
	 */
	int64_t Editor::MarkerNext(int64_t lineStart, int32_t markerMask)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerNext, static_cast<Scintilla::uptr_t>(lineStart), static_cast<Scintilla::sptr_t>(markerMask)));
	}

	/**
	 * Find the previous line before lineStart that includes a marker in mask.
	 */
	int64_t Editor::MarkerPrevious(int64_t lineStart, int32_t markerMask)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerPrevious, static_cast<Scintilla::uptr_t>(lineStart), static_cast<Scintilla::sptr_t>(markerMask)));
	}

	/**
	 * Define a marker from a pixmap.
	 */
	void Editor::MarkerDefinePixmapFromBuffer(int32_t markerNumber, Windows::Storage::Streams::IBuffer const &pixmap)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDefinePixmap, static_cast<Scintilla::uptr_t>(markerNumber), reinterpret_cast<Scintilla::sptr_t>(pixmap ? pixmap.data() : nullptr));
	}

	void Editor::MarkerDefinePixmap(int32_t markerNumber, hstring const &pixmap)
	{
		const auto pixmapUtf8{ to_string(pixmap) };
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDefinePixmap, static_cast<Scintilla::uptr_t>(markerNumber), reinterpret_cast<Scintilla::sptr_t>(pixmapUtf8.c_str()));
	}

	/**
	 * Add a set of markers to a line.
	 */
	void Editor::MarkerAddSet(int64_t line, int32_t markerSet)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerAddSet, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(markerSet));
	}

	/**
	 * Clear all the styles and make equivalent to the global default style.
	 */
	void Editor::StyleClearAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleClearAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Reset the default style to its state at startup
	 */
	void Editor::StyleResetDefault()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleResetDefault, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Use the default or platform-defined colour for an element.
	 */
	void Editor::ResetElementColour(WinUIEditor::Element const &element)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ResetElementColour, static_cast<Scintilla::uptr_t>(element), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the foreground colour of the main and additional selections and whether to use this setting.
	 */
	void Editor::SetSelFore(bool useSetting, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelFore, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set the background colour of the main and additional selections and whether to use this setting.
	 */
	void Editor::SetSelBack(bool useSetting, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelBack, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * When key+modifier combination keyDefinition is pressed perform sciCommand.
	 */
	void Editor::AssignCmdKey(int32_t keyDefinition, int32_t sciCommand)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AssignCmdKey, static_cast<Scintilla::uptr_t>(keyDefinition), static_cast<Scintilla::sptr_t>(sciCommand));
	}

	/**
	 * When key+modifier combination keyDefinition is pressed do nothing.
	 */
	void Editor::ClearCmdKey(int32_t keyDefinition)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearCmdKey, static_cast<Scintilla::uptr_t>(keyDefinition), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Drop all key mappings.
	 */
	void Editor::ClearAllCmdKeys()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearAllCmdKeys, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the styles for a segment of the document.
	 */
	void Editor::SetStylingExFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &styles)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetStylingEx, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(styles ? styles.data() : nullptr));
	}

	void Editor::SetStylingEx(int64_t length, hstring const &styles)
	{
		const auto stylesUtf8{ to_string(styles) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetStylingEx, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(stylesUtf8.c_str()));
	}

	/**
	 * Start a sequence of actions that is undone and redone as a unit.
	 * May be nested.
	 */
	void Editor::BeginUndoAction()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::BeginUndoAction, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * End a sequence of actions that is undone and redone as a unit.
	 */
	void Editor::EndUndoAction()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EndUndoAction, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Push one action onto undo history with no text
	 */
	void Editor::PushUndoActionType(int32_t type, int64_t pos)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PushUndoActionType, static_cast<Scintilla::uptr_t>(type), static_cast<Scintilla::sptr_t>(pos));
	}

	/**
	 * Set the text and length of the most recently pushed action
	 */
	void Editor::ChangeLastUndoActionTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ChangeLastUndoActionText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::ChangeLastUndoActionText(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::ChangeLastUndoActionText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Set the foreground colour of all whitespace and whether to use this setting.
	 */
	void Editor::SetWhitespaceFore(bool useSetting, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWhitespaceFore, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set the background colour of all whitespace and whether to use this setting.
	 */
	void Editor::SetWhitespaceBack(bool useSetting, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWhitespaceBack, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Display a auto-completion list.
	 * The lengthEntered parameter indicates how many characters before
	 * the caret should be used to provide context.
	 */
	void Editor::AutoCShowFromBuffer(int64_t lengthEntered, Windows::Storage::Streams::IBuffer const &itemList)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCShow, static_cast<Scintilla::uptr_t>(lengthEntered), reinterpret_cast<Scintilla::sptr_t>(itemList ? itemList.data() : nullptr));
	}

	void Editor::AutoCShow(int64_t lengthEntered, hstring const &itemList)
	{
		const auto itemListUtf8{ to_string(itemList) };
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCShow, static_cast<Scintilla::uptr_t>(lengthEntered), reinterpret_cast<Scintilla::sptr_t>(itemListUtf8.c_str()));
	}

	/**
	 * Remove the auto-completion list from the screen.
	 */
	void Editor::AutoCCancel()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCCancel, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is there an auto-completion list visible?
	 */
	bool Editor::AutoCActive()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCActive, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the position of the caret when the auto-completion list was displayed.
	 */
	int64_t Editor::AutoCPosStart()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCPosStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * User has selected an item so remove the list and insert the selection.
	 */
	void Editor::AutoCComplete()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCComplete, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Define a set of character that when typed cancel the auto-completion list.
	 */
	void Editor::AutoCStopsFromBuffer(Windows::Storage::Streams::IBuffer const &characterSet)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCStops, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characterSet ? characterSet.data() : nullptr));
	}

	void Editor::AutoCStops(hstring const &characterSet)
	{
		const auto characterSetUtf8{ to_string(characterSet) };
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCStops, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characterSetUtf8.c_str()));
	}

	/**
	 * Select the item in the auto-completion list that starts with a string.
	 */
	void Editor::AutoCSelectFromBuffer(Windows::Storage::Streams::IBuffer const &select)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSelect, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(select ? select.data() : nullptr));
	}

	void Editor::AutoCSelect(hstring const &select)
	{
		const auto selectUtf8{ to_string(select) };
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSelect, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(selectUtf8.c_str()));
	}

	/**
	 * Display a list of strings and send notification when user chooses one.
	 */
	void Editor::UserListShowFromBuffer(int32_t listType, Windows::Storage::Streams::IBuffer const &itemList)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::UserListShow, static_cast<Scintilla::uptr_t>(listType), reinterpret_cast<Scintilla::sptr_t>(itemList ? itemList.data() : nullptr));
	}

	void Editor::UserListShow(int32_t listType, hstring const &itemList)
	{
		const auto itemListUtf8{ to_string(itemList) };
		_editor.get()->PublicWndProc(Scintilla::Message::UserListShow, static_cast<Scintilla::uptr_t>(listType), reinterpret_cast<Scintilla::sptr_t>(itemListUtf8.c_str()));
	}

	/**
	 * Register an XPM image for use in autocompletion lists.
	 */
	void Editor::RegisterImageFromBuffer(int32_t type, Windows::Storage::Streams::IBuffer const &xpmData)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::RegisterImage, static_cast<Scintilla::uptr_t>(type), reinterpret_cast<Scintilla::sptr_t>(xpmData ? xpmData.data() : nullptr));
	}

	void Editor::RegisterImage(int32_t type, hstring const &xpmData)
	{
		const auto xpmDataUtf8{ to_string(xpmData) };
		_editor.get()->PublicWndProc(Scintilla::Message::RegisterImage, static_cast<Scintilla::uptr_t>(type), reinterpret_cast<Scintilla::sptr_t>(xpmDataUtf8.c_str()));
	}

	/**
	 * Clear all the registered XPM images.
	 */
	void Editor::ClearRegisteredImages()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearRegisteredImages, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Count characters between two positions.
	 */
	int64_t Editor::CountCharacters(int64_t start, int64_t end)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CountCharacters, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end)));
	}

	/**
	 * Count code units between two positions.
	 */
	int64_t Editor::CountCodeUnits(int64_t start, int64_t end)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CountCodeUnits, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end)));
	}

	/**
	 * Set caret to a position, while removing any existing selection.
	 */
	void Editor::SetEmptySelection(int64_t caret)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetEmptySelection, static_cast<Scintilla::uptr_t>(caret), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Find some text in the document.
	 */
	int64_t Editor::FindText(WinUIEditor::FindOption const &searchFlags, uint64_t ft)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::FindText, static_cast<Scintilla::uptr_t>(searchFlags), static_cast<Scintilla::sptr_t>(ft)));
	}

	/**
	 * Find some text in the document.
	 */
	int64_t Editor::FindTextFull(WinUIEditor::FindOption const &searchFlags, uint64_t ft)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::FindTextFull, static_cast<Scintilla::uptr_t>(searchFlags), static_cast<Scintilla::sptr_t>(ft)));
	}

	/**
	 * Draw the document into a display context such as a printer.
	 */
	int64_t Editor::FormatRange(bool draw, uint64_t fr)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::FormatRange, static_cast<Scintilla::uptr_t>(draw), static_cast<Scintilla::sptr_t>(fr)));
	}

	/**
	 * Draw the document into a display context such as a printer.
	 */
	int64_t Editor::FormatRangeFull(bool draw, uint64_t fr)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::FormatRangeFull, static_cast<Scintilla::uptr_t>(draw), static_cast<Scintilla::sptr_t>(fr)));
	}

	/**
	 * Retrieve the contents of a line.
	 * Returns the length of the line.
	 */
	int64_t Editor::GetLineWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLine, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetLine(int64_t line)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(line) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLine, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetLine, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Select a range of text.
	 */
	void Editor::SetSel(int64_t anchor, int64_t caret)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSel, static_cast<Scintilla::uptr_t>(anchor), static_cast<Scintilla::sptr_t>(caret));
	}

	/**
	 * Retrieve the selected text.
	 * Return the length of the text.
	 * Result is NUL-terminated.
	 */
	int64_t Editor::GetSelTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetSelText()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelText, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetSelText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve a range of text.
	 * Return the length of the text.
	 */
	int64_t Editor::GetTextRange(uint64_t tr)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTextRange, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(tr)));
	}

	/**
	 * Retrieve a range of text that can be past 2GB.
	 * Return the length of the text.
	 */
	int64_t Editor::GetTextRangeFull(uint64_t tr)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTextRangeFull, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(tr)));
	}

	/**
	 * Draw the selection either highlighted or in normal (non-highlighted) style.
	 */
	void Editor::HideSelection(bool hide)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HideSelection, static_cast<Scintilla::uptr_t>(hide), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the x value of the point in the window where a position is displayed.
	 */
	int32_t Editor::PointXFromPosition(int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::PointXFromPosition, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(pos)));
	}

	/**
	 * Retrieve the y value of the point in the window where a position is displayed.
	 */
	int32_t Editor::PointYFromPosition(int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::PointYFromPosition, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(pos)));
	}

	/**
	 * Retrieve the line containing a position.
	 */
	int64_t Editor::LineFromPosition(int64_t pos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::LineFromPosition, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the position at the start of a line.
	 */
	int64_t Editor::PositionFromLine(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionFromLine, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Scroll horizontally and vertically.
	 */
	void Editor::LineScroll(int64_t columns, int64_t lines)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineScroll, static_cast<Scintilla::uptr_t>(columns), static_cast<Scintilla::sptr_t>(lines));
	}

	/**
	 * Scroll vertically with allowance for wrapping.
	 */
	void Editor::ScrollVertical(int64_t docLine, int64_t subLine)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ScrollVertical, static_cast<Scintilla::uptr_t>(docLine), static_cast<Scintilla::sptr_t>(subLine));
	}

	/**
	 * Ensure the caret is visible.
	 */
	void Editor::ScrollCaret()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ScrollCaret, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Scroll the argument positions and the range between them into view giving
	 * priority to the primary position then the secondary position.
	 * This may be used to make a search match visible.
	 */
	void Editor::ScrollRange(int64_t secondary, int64_t primary)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ScrollRange, static_cast<Scintilla::uptr_t>(secondary), static_cast<Scintilla::sptr_t>(primary));
	}

	/**
	 * Replace the selected text with the argument text.
	 */
	void Editor::ReplaceSelFromBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ReplaceSel, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::ReplaceSel(hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::ReplaceSel, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Null operation.
	 */
	void Editor::Null()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Null, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Will a paste succeed?
	 */
	bool Editor::CanPaste()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::CanPaste, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Are there any undoable actions in the undo history?
	 */
	bool Editor::CanUndo()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::CanUndo, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Delete the undo history.
	 */
	void Editor::EmptyUndoBuffer()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EmptyUndoBuffer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Undo one action in the undo history.
	 */
	void Editor::Undo()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Undo, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Cut the selection to the clipboard.
	 */
	void Editor::Cut()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Cut, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Copy the selection to the clipboard.
	 */
	void Editor::Copy()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Copy, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Paste the contents of the clipboard into the document replacing the selection.
	 */
	void Editor::Paste()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Paste, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Clear the selection.
	 */
	void Editor::Clear()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Clear, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Replace the contents of the document with the argument text.
	 */
	void Editor::SetTextFromBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::SetText(hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Retrieve all the text in the document.
	 * Returns number of characters retrieved.
	 * Result is NUL-terminated.
	 */
	int64_t Editor::GetTextWriteBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetText(int64_t length)
	{
		if (length <= 0) return hstring{};
		const auto wParam{ static_cast<Scintilla::uptr_t>(length) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetText, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ std::min(sourceLength, static_cast<size_t>(length)) };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetText, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Sets both the start and end of the target in one call.
	 */
	void Editor::SetTargetRange(int64_t start, int64_t end)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetTargetRange, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end));
	}

	/**
	 * Make the target range start and end be the same as the selection range start and end.
	 */
	void Editor::TargetFromSelection()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::TargetFromSelection, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Sets the target to the whole document.
	 */
	void Editor::TargetWholeDocument()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::TargetWholeDocument, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Replace the target text with the argument text.
	 * Text is counted so it can contain NULs.
	 * Returns the length of the replacement text.
	 */
	int64_t Editor::ReplaceTargetFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ReplaceTarget, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int64_t Editor::ReplaceTarget(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ReplaceTarget, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Replace the target text with the argument text after \d processing.
	 * Text is counted so it can contain NULs.
	 * Looks for \d where d is between 1 and 9 and replaces these with the strings
	 * matched in the last search operation which were surrounded by \( and \).
	 * Returns the length of the replacement text including any change
	 * caused by processing the \d patterns.
	 */
	int64_t Editor::ReplaceTargetREFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ReplaceTargetRE, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int64_t Editor::ReplaceTargetRE(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ReplaceTargetRE, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Replace the target text with the argument text but ignore prefix and suffix that
	 * are the same as current.
	 */
	int64_t Editor::ReplaceTargetMinimalFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ReplaceTargetMinimal, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int64_t Editor::ReplaceTargetMinimal(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ReplaceTargetMinimal, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Search for a counted string in the target and set the target to the found
	 * range. Text is counted so it can contain NULs.
	 * Returns start of found range or -1 for failure in which case target is not moved.
	 */
	int64_t Editor::SearchInTargetFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::SearchInTarget, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int64_t Editor::SearchInTarget(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::SearchInTarget, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Show a call tip containing a definition near position pos.
	 */
	void Editor::CallTipShowFromBuffer(int64_t pos, Windows::Storage::Streams::IBuffer const &definition)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipShow, static_cast<Scintilla::uptr_t>(pos), reinterpret_cast<Scintilla::sptr_t>(definition ? definition.data() : nullptr));
	}

	void Editor::CallTipShow(int64_t pos, hstring const &definition)
	{
		const auto definitionUtf8{ to_string(definition) };
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipShow, static_cast<Scintilla::uptr_t>(pos), reinterpret_cast<Scintilla::sptr_t>(definitionUtf8.c_str()));
	}

	/**
	 * Remove the call tip from the screen.
	 */
	void Editor::CallTipCancel()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipCancel, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Is there an active call tip?
	 */
	bool Editor::CallTipActive()
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::CallTipActive, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the position where the caret was before displaying the call tip.
	 */
	int64_t Editor::CallTipPosStart()
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CallTipPosStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Highlight a segment of the definition.
	 */
	void Editor::CallTipSetHlt(int64_t highlightStart, int64_t highlightEnd)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipSetHlt, static_cast<Scintilla::uptr_t>(highlightStart), static_cast<Scintilla::sptr_t>(highlightEnd));
	}

	/**
	 * Find the display line of a document line taking hidden lines into account.
	 */
	int64_t Editor::VisibleFromDocLine(int64_t docLine)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::VisibleFromDocLine, static_cast<Scintilla::uptr_t>(docLine), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Find the document line of a display line taking hidden lines into account.
	 */
	int64_t Editor::DocLineFromVisible(int64_t displayLine)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::DocLineFromVisible, static_cast<Scintilla::uptr_t>(displayLine), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * The number of display lines needed to wrap a document line
	 */
	int64_t Editor::WrapCount(int64_t docLine)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::WrapCount, static_cast<Scintilla::uptr_t>(docLine), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Make a range of lines visible.
	 */
	void Editor::ShowLines(int64_t lineStart, int64_t lineEnd)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ShowLines, static_cast<Scintilla::uptr_t>(lineStart), static_cast<Scintilla::sptr_t>(lineEnd));
	}

	/**
	 * Make a range of lines invisible.
	 */
	void Editor::HideLines(int64_t lineStart, int64_t lineEnd)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HideLines, static_cast<Scintilla::uptr_t>(lineStart), static_cast<Scintilla::sptr_t>(lineEnd));
	}

	/**
	 * Switch a header line between expanded and contracted.
	 */
	void Editor::ToggleFold(int64_t line)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ToggleFold, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Switch a header line between expanded and contracted and show some text after the line.
	 */
	void Editor::ToggleFoldShowTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ToggleFoldShowText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::ToggleFoldShowText(int64_t line, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::ToggleFoldShowText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Set the default fold display text.
	 */
	void Editor::SetDefaultFoldDisplayTextFromBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetDefaultFoldDisplayText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::SetDefaultFoldDisplayText(hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetDefaultFoldDisplayText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Get the default fold display text.
	 */
	int32_t Editor::GetDefaultFoldDisplayTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetDefaultFoldDisplayText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetDefaultFoldDisplayText()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetDefaultFoldDisplayText, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetDefaultFoldDisplayText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Expand or contract a fold header.
	 */
	void Editor::FoldLine(int64_t line, WinUIEditor::FoldAction const &action)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FoldLine, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(action));
	}

	/**
	 * Expand or contract a fold header and its children.
	 */
	void Editor::FoldChildren(int64_t line, WinUIEditor::FoldAction const &action)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FoldChildren, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(action));
	}

	/**
	 * Expand a fold header and all children. Use the level argument instead of the line's current level.
	 */
	void Editor::ExpandChildren(int64_t line, WinUIEditor::FoldLevel const &level)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ExpandChildren, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(level));
	}

	/**
	 * Expand or contract all fold headers.
	 */
	void Editor::FoldAll(WinUIEditor::FoldAction const &action)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FoldAll, static_cast<Scintilla::uptr_t>(action), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Ensure a particular line is visible by expanding any header line hiding it.
	 */
	void Editor::EnsureVisible(int64_t line)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EnsureVisible, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Ensure a particular line is visible by expanding any header line hiding it.
	 * Use the currently set visibility policy to determine which range to display.
	 */
	void Editor::EnsureVisibleEnforcePolicy(int64_t line)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EnsureVisibleEnforcePolicy, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Get position of start of word.
	 */
	int64_t Editor::WordStartPosition(int64_t pos, bool onlyWordCharacters)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::WordStartPosition, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(onlyWordCharacters)));
	}

	/**
	 * Get position of end of word.
	 */
	int64_t Editor::WordEndPosition(int64_t pos, bool onlyWordCharacters)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::WordEndPosition, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(onlyWordCharacters)));
	}

	/**
	 * Is the range start..end considered a word?
	 */
	bool Editor::IsRangeWord(int64_t start, int64_t end)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::IsRangeWord, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end)));
	}

	/**
	 * Measure the pixel width of some text in a particular style.
	 * NUL terminated text argument.
	 * Does not handle tab or control characters.
	 */
	int32_t Editor::TextWidthFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::TextWidth, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int32_t Editor::TextWidth(int32_t style, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::TextWidth, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Retrieve the height of a particular line of text in pixels.
	 */
	int32_t Editor::TextHeight(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::TextHeight, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Append a string to the end of the document without changing the selection.
	 */
	void Editor::AppendTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AppendText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::AppendText(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::AppendText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Join the lines in the target.
	 */
	void Editor::LinesJoin()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LinesJoin, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Split the lines in the target into lines that are less wide than pixelWidth
	 * where possible.
	 */
	void Editor::LinesSplit(int32_t pixelWidth)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LinesSplit, static_cast<Scintilla::uptr_t>(pixelWidth), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set one of the colours used as a chequerboard pattern in the fold margin
	 */
	void Editor::SetFoldMarginColour(bool useSetting, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFoldMarginColour, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the other colour used as a chequerboard pattern in the fold margin
	 */
	void Editor::SetFoldMarginHiColour(bool useSetting, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFoldMarginHiColour, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Move caret down one line.
	 */
	void Editor::LineDown()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineDown, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret down one line extending selection to new caret position.
	 */
	void Editor::LineDownExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineDownExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret up one line.
	 */
	void Editor::LineUp()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineUp, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret up one line extending selection to new caret position.
	 */
	void Editor::LineUpExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineUpExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one character.
	 */
	void Editor::CharLeft()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CharLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one character extending selection to new caret position.
	 */
	void Editor::CharLeftExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CharLeftExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one character.
	 */
	void Editor::CharRight()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CharRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one character extending selection to new caret position.
	 */
	void Editor::CharRightExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CharRightExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one word.
	 */
	void Editor::WordLeft()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one word extending selection to new caret position.
	 */
	void Editor::WordLeftExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordLeftExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one word.
	 */
	void Editor::WordRight()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one word extending selection to new caret position.
	 */
	void Editor::WordRightExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordRightExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position on line.
	 */
	void Editor::Home()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Home, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position on line extending selection to new caret position.
	 */
	void Editor::HomeExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HomeExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position on line.
	 */
	void Editor::LineEnd()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position on line extending selection to new caret position.
	 */
	void Editor::LineEndExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEndExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position in document.
	 */
	void Editor::DocumentStart()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DocumentStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position in document extending selection to new caret position.
	 */
	void Editor::DocumentStartExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DocumentStartExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position in document.
	 */
	void Editor::DocumentEnd()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DocumentEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position in document extending selection to new caret position.
	 */
	void Editor::DocumentEndExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DocumentEndExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret one page up.
	 */
	void Editor::PageUp()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PageUp, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret one page up extending selection to new caret position.
	 */
	void Editor::PageUpExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PageUpExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret one page down.
	 */
	void Editor::PageDown()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PageDown, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret one page down extending selection to new caret position.
	 */
	void Editor::PageDownExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PageDownExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Switch from insert to overtype mode or the reverse.
	 */
	void Editor::EditToggleOvertype()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EditToggleOvertype, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Cancel any modes such as call tip or auto-completion list display.
	 */
	void Editor::Cancel()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Cancel, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete the selection or if no selection, the character before the caret.
	 */
	void Editor::DeleteBack()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DeleteBack, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * If selection is empty or all on one line replace the selection with a tab character.
	 * If more than one line selected, indent the lines.
	 */
	void Editor::Tab()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Tab, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Indent the current and selected lines.
	 */
	void Editor::LineIndent()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineIndent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * If selection is empty or all on one line dedent the line if caret is at start, else move caret.
	 * If more than one line selected, dedent the lines.
	 */
	void Editor::BackTab()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::BackTab, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Dedent the current and selected lines.
	 */
	void Editor::LineDedent()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineDedent, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Insert a new line, may use a CRLF, CR or LF depending on EOL mode.
	 */
	void Editor::NewLine()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::NewLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Insert a Form Feed character.
	 */
	void Editor::FormFeed()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FormFeed, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to before first visible character on line.
	 * If already there move to first character on line.
	 */
	void Editor::VCHome()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHome, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like VCHome but extending selection to new caret position.
	 */
	void Editor::VCHomeExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHomeExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Magnify the displayed text by increasing the sizes by 1 point.
	 */
	void Editor::ZoomIn()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ZoomIn, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Make the displayed text smaller by decreasing the sizes by 1 point.
	 */
	void Editor::ZoomOut()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ZoomOut, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete the word to the left of the caret.
	 */
	void Editor::DelWordLeft()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DelWordLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete the word to the right of the caret.
	 */
	void Editor::DelWordRight()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DelWordRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete the word to the right of the caret, but not the trailing non-word characters.
	 */
	void Editor::DelWordRightEnd()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DelWordRightEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Cut the line containing the caret.
	 */
	void Editor::LineCut()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineCut, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete the line containing the caret.
	 */
	void Editor::LineDelete()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineDelete, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Switch the current line with the previous.
	 */
	void Editor::LineTranspose()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineTranspose, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Reverse order of selected lines.
	 */
	void Editor::LineReverse()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineReverse, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Duplicate the current line.
	 */
	void Editor::LineDuplicate()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineDuplicate, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Transform the selection to lower case.
	 */
	void Editor::LowerCase()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LowerCase, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Transform the selection to upper case.
	 */
	void Editor::UpperCase()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::UpperCase, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Scroll the document down, keeping the caret visible.
	 */
	void Editor::LineScrollDown()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineScrollDown, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Scroll the document up, keeping the caret visible.
	 */
	void Editor::LineScrollUp()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineScrollUp, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete the selection or if no selection, the character before the caret.
	 * Will not delete the character before at the start of a line.
	 */
	void Editor::DeleteBackNotLine()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DeleteBackNotLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position on display line.
	 */
	void Editor::HomeDisplay()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HomeDisplay, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position on display line extending selection to
	 * new caret position.
	 */
	void Editor::HomeDisplayExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HomeDisplayExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position on display line.
	 */
	void Editor::LineEndDisplay()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEndDisplay, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position on display line extending selection to new
	 * caret position.
	 */
	void Editor::LineEndDisplayExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEndDisplayExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like Home but when word-wrap is enabled goes first to start of display line
	 * HomeDisplay, then to start of document line Home.
	 */
	void Editor::HomeWrap()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HomeWrap, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like HomeExtend but when word-wrap is enabled extends first to start of display line
	 * HomeDisplayExtend, then to start of document line HomeExtend.
	 */
	void Editor::HomeWrapExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HomeWrapExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like LineEnd but when word-wrap is enabled goes first to end of display line
	 * LineEndDisplay, then to start of document line LineEnd.
	 */
	void Editor::LineEndWrap()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEndWrap, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like LineEndExtend but when word-wrap is enabled extends first to end of display line
	 * LineEndDisplayExtend, then to start of document line LineEndExtend.
	 */
	void Editor::LineEndWrapExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEndWrapExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like VCHome but when word-wrap is enabled goes first to start of display line
	 * VCHomeDisplay, then behaves like VCHome.
	 */
	void Editor::VCHomeWrap()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHomeWrap, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like VCHomeExtend but when word-wrap is enabled extends first to start of display line
	 * VCHomeDisplayExtend, then behaves like VCHomeExtend.
	 */
	void Editor::VCHomeWrapExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHomeWrapExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Copy the line containing the caret.
	 */
	void Editor::LineCopy()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineCopy, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move the caret inside current view if it's not there already.
	 */
	void Editor::MoveCaretInsideView()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MoveCaretInsideView, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * How many characters are on a line, including end of line characters?
	 */
	int64_t Editor::LineLength(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::LineLength, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Highlight the characters at two positions.
	 */
	void Editor::BraceHighlight(int64_t posA, int64_t posB)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::BraceHighlight, static_cast<Scintilla::uptr_t>(posA), static_cast<Scintilla::sptr_t>(posB));
	}

	/**
	 * Use specified indicator to highlight matching braces instead of changing their style.
	 */
	void Editor::BraceHighlightIndicator(bool useSetting, int32_t indicator)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::BraceHighlightIndicator, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(indicator));
	}

	/**
	 * Highlight the character at a position indicating there is no matching brace.
	 */
	void Editor::BraceBadLight(int64_t pos)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::BraceBadLight, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Use specified indicator to highlight non matching brace instead of changing its style.
	 */
	void Editor::BraceBadLightIndicator(bool useSetting, int32_t indicator)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::BraceBadLightIndicator, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(indicator));
	}

	/**
	 * Find the position of a matching brace or INVALID_POSITION if no match.
	 * The maxReStyle must be 0 for now. It may be defined in a future release.
	 */
	int64_t Editor::BraceMatch(int64_t pos, int32_t maxReStyle)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::BraceMatch, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(maxReStyle)));
	}

	/**
	 * Similar to BraceMatch, but matching starts at the explicit start position.
	 */
	int64_t Editor::BraceMatchNext(int64_t pos, int64_t startPos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::BraceMatchNext, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(startPos)));
	}

	/**
	 * Add a new vertical edge to the view.
	 */
	void Editor::MultiEdgeAddLine(int64_t column, int32_t edgeColour)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MultiEdgeAddLine, static_cast<Scintilla::uptr_t>(column), static_cast<Scintilla::sptr_t>(edgeColour));
	}

	/**
	 * Clear all vertical edges.
	 */
	void Editor::MultiEdgeClearAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MultiEdgeClearAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Sets the current caret position to be the search anchor.
	 */
	void Editor::SearchAnchor()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SearchAnchor, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Find some text starting at the search anchor.
	 * Does not ensure the selection is visible.
	 */
	int64_t Editor::SearchNextFromBuffer(WinUIEditor::FindOption const &searchFlags, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::SearchNext, static_cast<Scintilla::uptr_t>(searchFlags), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int64_t Editor::SearchNext(WinUIEditor::FindOption const &searchFlags, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::SearchNext, static_cast<Scintilla::uptr_t>(searchFlags), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Find some text starting at the search anchor and moving backwards.
	 * Does not ensure the selection is visible.
	 */
	int64_t Editor::SearchPrevFromBuffer(WinUIEditor::FindOption const &searchFlags, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::SearchPrev, static_cast<Scintilla::uptr_t>(searchFlags), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	int64_t Editor::SearchPrev(WinUIEditor::FindOption const &searchFlags, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::SearchPrev, static_cast<Scintilla::uptr_t>(searchFlags), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str())));
	}

	/**
	 * Set whether a pop up menu is displayed automatically when the user presses
	 * the wrong mouse button on certain areas.
	 */
	void Editor::UsePopUp(WinUIEditor::PopUp const &popUpMode)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::UsePopUp, static_cast<Scintilla::uptr_t>(popUpMode), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Create a new document object.
	 * Starts with reference count of 1 and not selected into editor.
	 */
	uint64_t Editor::CreateDocument(int64_t bytes, WinUIEditor::DocumentOption const &documentOptions)
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CreateDocument, static_cast<Scintilla::uptr_t>(bytes), static_cast<Scintilla::sptr_t>(documentOptions)));
	}

	/**
	 * Extend life of document.
	 */
	void Editor::AddRefDocument(uint64_t doc)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AddRefDocument, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(doc));
	}

	/**
	 * Release a reference to the document, deleting document if it fades to black.
	 */
	void Editor::ReleaseDocument(uint64_t doc)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ReleaseDocument, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(doc));
	}

	/**
	 * Move to the previous change in capitalisation.
	 */
	void Editor::WordPartLeft()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordPartLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move to the previous change in capitalisation extending selection
	 * to new caret position.
	 */
	void Editor::WordPartLeftExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordPartLeftExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move to the change next in capitalisation.
	 */
	void Editor::WordPartRight()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordPartRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move to the next change in capitalisation extending selection
	 * to new caret position.
	 */
	void Editor::WordPartRightExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordPartRightExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the way the display area is determined when a particular line
	 * is to be moved to by Find, FindNext, GotoLine, etc.
	 */
	void Editor::SetVisiblePolicy(WinUIEditor::VisiblePolicy const &visiblePolicy, int32_t visibleSlop)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetVisiblePolicy, static_cast<Scintilla::uptr_t>(visiblePolicy), static_cast<Scintilla::sptr_t>(visibleSlop));
	}

	/**
	 * Delete back from the current position to the start of the line.
	 */
	void Editor::DelLineLeft()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DelLineLeft, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Delete forwards from the current position to the end of the line.
	 */
	void Editor::DelLineRight()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DelLineRight, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the last x chosen value to be the caret x position.
	 */
	void Editor::ChooseCaretX()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ChooseCaretX, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the focus to this Scintilla widget.
	 */
	void Editor::GrabFocus()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::GrabFocus, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the way the caret is kept visible when going sideways.
	 * The exclusion zone is given in pixels.
	 */
	void Editor::SetXCaretPolicy(WinUIEditor::CaretPolicy const &caretPolicy, int32_t caretSlop)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetXCaretPolicy, static_cast<Scintilla::uptr_t>(caretPolicy), static_cast<Scintilla::sptr_t>(caretSlop));
	}

	/**
	 * Set the way the line the caret is on is kept visible.
	 * The exclusion zone is given in lines.
	 */
	void Editor::SetYCaretPolicy(WinUIEditor::CaretPolicy const &caretPolicy, int32_t caretSlop)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetYCaretPolicy, static_cast<Scintilla::uptr_t>(caretPolicy), static_cast<Scintilla::sptr_t>(caretSlop));
	}

	/**
	 * Move caret down one paragraph (delimited by empty lines).
	 */
	void Editor::ParaDown()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ParaDown, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Extend selection down one paragraph (delimited by empty lines).
	 */
	void Editor::ParaDownExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ParaDownExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret up one paragraph (delimited by empty lines).
	 */
	void Editor::ParaUp()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ParaUp, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Extend selection up one paragraph (delimited by empty lines).
	 */
	void Editor::ParaUpExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ParaUpExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Given a valid document position, return the previous position taking code
	 * page into account. Returns 0 if passed 0.
	 */
	int64_t Editor::PositionBefore(int64_t pos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionBefore, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Given a valid document position, return the next position taking code
	 * page into account. Maximum value returned is the last position in the document.
	 */
	int64_t Editor::PositionAfter(int64_t pos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionAfter, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Given a valid document position, return a position that differs in a number
	 * of characters. Returned value is always between 0 and last position in document.
	 */
	int64_t Editor::PositionRelative(int64_t pos, int64_t relative)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionRelative, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(relative)));
	}

	/**
	 * Given a valid document position, return a position that differs in a number
	 * of UTF-16 code units. Returned value is always between 0 and last position in document.
	 * The result may point half way (2 bytes) inside a non-BMP character.
	 */
	int64_t Editor::PositionRelativeCodeUnits(int64_t pos, int64_t relative)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PositionRelativeCodeUnits, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(relative)));
	}

	/**
	 * Copy a range of text to the clipboard. Positions are clipped into the document.
	 */
	void Editor::CopyRange(int64_t start, int64_t end)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CopyRange, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end));
	}

	/**
	 * Copy argument text to the clipboard.
	 */
	void Editor::CopyTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CopyText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::CopyText(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::CopyText, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Set the selection mode to stream (SC_SEL_STREAM) or rectangular (SC_SEL_RECTANGLE/SC_SEL_THIN) or
	 * by lines (SC_SEL_LINES) without changing MoveExtendsSelection.
	 */
	void Editor::ChangeSelectionMode(WinUIEditor::SelectionMode const &selectionMode)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ChangeSelectionMode, static_cast<Scintilla::uptr_t>(selectionMode), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the position of the start of the selection at the given line (INVALID_POSITION if no selection on this line).
	 */
	int64_t Editor::GetLineSelStartPosition(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineSelStartPosition, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the position of the end of the selection at the given line (INVALID_POSITION if no selection on this line).
	 */
	int64_t Editor::GetLineSelEndPosition(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineSelEndPosition, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Move caret down one line, extending rectangular selection to new caret position.
	 */
	void Editor::LineDownRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineDownRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret up one line, extending rectangular selection to new caret position.
	 */
	void Editor::LineUpRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineUpRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one character, extending rectangular selection to new caret position.
	 */
	void Editor::CharLeftRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CharLeftRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one character, extending rectangular selection to new caret position.
	 */
	void Editor::CharRightRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CharRightRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to first position on line, extending rectangular selection to new caret position.
	 */
	void Editor::HomeRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::HomeRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to before first visible character on line.
	 * If already there move to first character on line.
	 * In either case, extend rectangular selection to new caret position.
	 */
	void Editor::VCHomeRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHomeRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to last position on line, extending rectangular selection to new caret position.
	 */
	void Editor::LineEndRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::LineEndRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret one page up, extending rectangular selection to new caret position.
	 */
	void Editor::PageUpRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PageUpRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret one page down, extending rectangular selection to new caret position.
	 */
	void Editor::PageDownRectExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::PageDownRectExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to top of page, or one page up if already at top of page.
	 */
	void Editor::StutteredPageUp()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StutteredPageUp, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to top of page, or one page up if already at top of page, extending selection to new caret position.
	 */
	void Editor::StutteredPageUpExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StutteredPageUpExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to bottom of page, or one page down if already at bottom of page.
	 */
	void Editor::StutteredPageDown()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StutteredPageDown, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to bottom of page, or one page down if already at bottom of page, extending selection to new caret position.
	 */
	void Editor::StutteredPageDownExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StutteredPageDownExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one word, position cursor at end of word.
	 */
	void Editor::WordLeftEnd()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordLeftEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret left one word, position cursor at end of word, extending selection to new caret position.
	 */
	void Editor::WordLeftEndExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordLeftEndExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one word, position cursor at end of word.
	 */
	void Editor::WordRightEnd()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordRightEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret right one word, position cursor at end of word, extending selection to new caret position.
	 */
	void Editor::WordRightEndExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::WordRightEndExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Reset the set of characters for whitespace and word characters to the defaults.
	 */
	void Editor::SetCharsDefault()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCharsDefault, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Enlarge the document to a particular size of text bytes.
	 */
	void Editor::Allocate(int64_t bytes)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Allocate, static_cast<Scintilla::uptr_t>(bytes), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Returns the target converted to UTF8.
	 * Return the length in bytes.
	 */
	int64_t Editor::TargetAsUTF8WriteBuffer(Windows::Storage::Streams::IBuffer const &s)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::TargetAsUTF8, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(s ? s.data() : nullptr)));
	}

	hstring Editor::TargetAsUTF8()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::TargetAsUTF8, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::TargetAsUTF8, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Set the length of the utf8 argument for calling EncodedFromUTF8.
	 * Set to -1 and the string will be measured to the first nul.
	 */
	void Editor::SetLengthForEncode(int64_t bytes)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetLengthForEncode, static_cast<Scintilla::uptr_t>(bytes), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Translates a UTF8 string into the document encoding.
	 * Return the length of the result in bytes.
	 * On error return 0.
	 */
	int64_t Editor::EncodedFromUTF8WriteBuffer(Windows::Storage::Streams::IBuffer const &utf8, Windows::Storage::Streams::IBuffer const &encoded)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::EncodedFromUTF8, reinterpret_cast<Scintilla::uptr_t>(utf8 ? utf8.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(encoded ? encoded.data() : nullptr)));
	}

	hstring Editor::EncodedFromUTF8(hstring const &utf8)
	{
		const auto utf8Utf8{ to_string(utf8) };
		const auto wParam{ reinterpret_cast<Scintilla::uptr_t>(utf8Utf8.c_str()) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::EncodedFromUTF8, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::EncodedFromUTF8, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Find the position of a column on a line taking into account tabs and
	 * multi-byte characters. If beyond end of line, return line end position.
	 */
	int64_t Editor::FindColumn(int64_t line, int64_t column)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::FindColumn, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(column)));
	}

	/**
	 * Switch between sticky and non-sticky: meant to be bound to a key.
	 */
	void Editor::ToggleCaretSticky()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ToggleCaretSticky, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Replace the selection with text like a rectangular paste.
	 */
	void Editor::ReplaceRectangularFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ReplaceRectangular, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::ReplaceRectangular(int64_t length, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::ReplaceRectangular, static_cast<Scintilla::uptr_t>(length), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Duplicate the selection. If selection empty duplicate the line containing the caret.
	 */
	void Editor::SelectionDuplicate()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SelectionDuplicate, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Turn a indicator on over a range.
	 */
	void Editor::IndicatorFillRange(int64_t start, int64_t lengthFill)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicatorFillRange, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(lengthFill));
	}

	/**
	 * Turn a indicator off over a range.
	 */
	void Editor::IndicatorClearRange(int64_t start, int64_t lengthClear)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicatorClearRange, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(lengthClear));
	}

	/**
	 * Are any indicators present at pos?
	 */
	int32_t Editor::IndicatorAllOnFor(int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicatorAllOnFor, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * What value does a particular indicator have at a position?
	 */
	int32_t Editor::IndicatorValueAt(int32_t indicator, int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicatorValueAt, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(pos)));
	}

	/**
	 * Where does a particular indicator start?
	 */
	int64_t Editor::IndicatorStart(int32_t indicator, int64_t pos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicatorStart, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(pos)));
	}

	/**
	 * Where does a particular indicator end?
	 */
	int64_t Editor::IndicatorEnd(int32_t indicator, int64_t pos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicatorEnd, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(pos)));
	}

	/**
	 * Copy the selection, if selection empty copy the line with the caret
	 */
	void Editor::CopyAllowLine()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CopyAllowLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Cut the selection, if selection empty cut the line with the caret
	 */
	void Editor::CutAllowLine()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CutAllowLine, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Which symbol was defined for markerNumber with MarkerDefine
	 */
	WinUIEditor::MarkerSymbol Editor::MarkerSymbolDefined(int32_t markerNumber)
	{
		return static_cast<WinUIEditor::MarkerSymbol>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerSymbolDefined, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Clear the margin text on all lines
	 */
	void Editor::MarginTextClearAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarginTextClearAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Clear the annotations from all lines
	 */
	void Editor::AnnotationClearAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationClearAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Release all extended (>255) style numbers
	 */
	void Editor::ReleaseAllExtendedStyles()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ReleaseAllExtendedStyles, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Allocate some extended (>255) style numbers and return the start of the range
	 */
	int32_t Editor::AllocateExtendedStyles(int32_t numberStyles)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AllocateExtendedStyles, static_cast<Scintilla::uptr_t>(numberStyles), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Add a container action to the undo stack
	 */
	void Editor::AddUndoAction(int32_t token, WinUIEditor::UndoFlags const &flags)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AddUndoAction, static_cast<Scintilla::uptr_t>(token), static_cast<Scintilla::sptr_t>(flags));
	}

	/**
	 * Find the position of a character from a point within the window.
	 */
	int64_t Editor::CharPositionFromPoint(int32_t x, int32_t y)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CharPositionFromPoint, static_cast<Scintilla::uptr_t>(x), static_cast<Scintilla::sptr_t>(y)));
	}

	/**
	 * Find the position of a character from a point within the window.
	 * Return INVALID_POSITION if not close to text.
	 */
	int64_t Editor::CharPositionFromPointClose(int32_t x, int32_t y)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CharPositionFromPointClose, static_cast<Scintilla::uptr_t>(x), static_cast<Scintilla::sptr_t>(y)));
	}

	/**
	 * Clear selections to a single empty stream selection
	 */
	void Editor::ClearSelections()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearSelections, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set a simple selection
	 */
	void Editor::SetSelection(int64_t caret, int64_t anchor)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelection, static_cast<Scintilla::uptr_t>(caret), static_cast<Scintilla::sptr_t>(anchor));
	}

	/**
	 * Add a selection
	 */
	void Editor::AddSelection(int64_t caret, int64_t anchor)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AddSelection, static_cast<Scintilla::uptr_t>(caret), static_cast<Scintilla::sptr_t>(anchor));
	}

	/**
	 * Find the selection index for a point. -1 when not at a selection.
	 */
	int32_t Editor::SelectionFromPoint(int32_t x, int32_t y)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::SelectionFromPoint, static_cast<Scintilla::uptr_t>(x), static_cast<Scintilla::sptr_t>(y)));
	}

	/**
	 * Drop one selection
	 */
	void Editor::DropSelectionN(int32_t selection)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::DropSelectionN, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the main selection to the next selection.
	 */
	void Editor::RotateSelection()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::RotateSelection, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Swap that caret and anchor of the main selection.
	 */
	void Editor::SwapMainAnchorCaret()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SwapMainAnchorCaret, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Add the next occurrence of the main selection to the set of selections as main.
	 * If the current selection is empty then select word around caret.
	 */
	void Editor::MultipleSelectAddNext()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MultipleSelectAddNext, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Add each occurrence of the main selection in the target to the set of selections.
	 * If the current selection is empty then select word around caret.
	 */
	void Editor::MultipleSelectAddEach()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MultipleSelectAddEach, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Indicate that the internal state of a lexer has changed over a range and therefore
	 * there may be a need to redraw.
	 */
	int32_t Editor::ChangeLexerState(int64_t start, int64_t end)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::ChangeLexerState, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end)));
	}

	/**
	 * Find the next line at or after lineStart that is a contracted fold header line.
	 * Return -1 when no more lines.
	 */
	int64_t Editor::ContractedFoldNext(int64_t lineStart)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::ContractedFoldNext, static_cast<Scintilla::uptr_t>(lineStart), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Centre current line in window.
	 */
	void Editor::VerticalCentreCaret()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VerticalCentreCaret, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move the selected lines up one line, shifting the line above after the selection
	 */
	void Editor::MoveSelectedLinesUp()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MoveSelectedLinesUp, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move the selected lines down one line, shifting the line below before the selection
	 */
	void Editor::MoveSelectedLinesDown()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MoveSelectedLinesDown, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Define a marker from RGBA data.
	 * It has the width and height from RGBAImageSetWidth/Height
	 */
	void Editor::MarkerDefineRGBAImageFromBuffer(int32_t markerNumber, Windows::Storage::Streams::IBuffer const &pixels)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDefineRGBAImage, static_cast<Scintilla::uptr_t>(markerNumber), reinterpret_cast<Scintilla::sptr_t>(pixels ? pixels.data() : nullptr));
	}

	void Editor::MarkerDefineRGBAImage(int32_t markerNumber, hstring const &pixels)
	{
		const auto pixelsUtf8{ to_string(pixels) };
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerDefineRGBAImage, static_cast<Scintilla::uptr_t>(markerNumber), reinterpret_cast<Scintilla::sptr_t>(pixelsUtf8.c_str()));
	}

	/**
	 * Register an RGBA image for use in autocompletion lists.
	 * It has the width and height from RGBAImageSetWidth/Height
	 */
	void Editor::RegisterRGBAImageFromBuffer(int32_t type, Windows::Storage::Streams::IBuffer const &pixels)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::RegisterRGBAImage, static_cast<Scintilla::uptr_t>(type), reinterpret_cast<Scintilla::sptr_t>(pixels ? pixels.data() : nullptr));
	}

	void Editor::RegisterRGBAImage(int32_t type, hstring const &pixels)
	{
		const auto pixelsUtf8{ to_string(pixels) };
		_editor.get()->PublicWndProc(Scintilla::Message::RegisterRGBAImage, static_cast<Scintilla::uptr_t>(type), reinterpret_cast<Scintilla::sptr_t>(pixelsUtf8.c_str()));
	}

	/**
	 * Scroll to start of document.
	 */
	void Editor::ScrollToStart()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ScrollToStart, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Scroll to end of document.
	 */
	void Editor::ScrollToEnd()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ScrollToEnd, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Create an ILoader*.
	 */
	uint64_t Editor::CreateLoader(int64_t bytes, WinUIEditor::DocumentOption const &documentOptions)
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::CreateLoader, static_cast<Scintilla::uptr_t>(bytes), static_cast<Scintilla::sptr_t>(documentOptions)));
	}

	/**
	 * On macOS, show a find indicator.
	 */
	void Editor::FindIndicatorShow(int64_t start, int64_t end)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FindIndicatorShow, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end));
	}

	/**
	 * On macOS, flash a find indicator, then fade out.
	 */
	void Editor::FindIndicatorFlash(int64_t start, int64_t end)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FindIndicatorFlash, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end));
	}

	/**
	 * On macOS, hide the find indicator.
	 */
	void Editor::FindIndicatorHide()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FindIndicatorHide, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Move caret to before first visible character on display line.
	 * If already there move to first character on display line.
	 */
	void Editor::VCHomeDisplay()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHomeDisplay, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Like VCHomeDisplay but extending selection to new caret position.
	 */
	void Editor::VCHomeDisplayExtend()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::VCHomeDisplayExtend, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Remove a character representation.
	 */
	void Editor::ClearRepresentationFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearRepresentation, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), static_cast<Scintilla::sptr_t>(0));
	}

	void Editor::ClearRepresentation(hstring const &encodedCharacter)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		_editor.get()->PublicWndProc(Scintilla::Message::ClearRepresentation, reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Clear representations to default.
	 */
	void Editor::ClearAllRepresentations()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ClearAllRepresentations, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Clear the end of annotations from all lines
	 */
	void Editor::EOLAnnotationClearAll()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationClearAll, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Request line character index be created or its use count increased.
	 */
	void Editor::AllocateLineCharacterIndex(WinUIEditor::LineCharacterIndexType const &lineCharacterIndex)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AllocateLineCharacterIndex, static_cast<Scintilla::uptr_t>(lineCharacterIndex), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Decrease use count of line character index and remove if 0.
	 */
	void Editor::ReleaseLineCharacterIndex(WinUIEditor::LineCharacterIndexType const &lineCharacterIndex)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::ReleaseLineCharacterIndex, static_cast<Scintilla::uptr_t>(lineCharacterIndex), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the document line containing a position measured in index units.
	 */
	int64_t Editor::LineFromIndexPosition(int64_t pos, WinUIEditor::LineCharacterIndexType const &lineCharacterIndex)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::LineFromIndexPosition, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(lineCharacterIndex)));
	}

	/**
	 * Retrieve the position measured in index units at the start of a document line.
	 */
	int64_t Editor::IndexPositionFromLine(int64_t line, WinUIEditor::LineCharacterIndexType const &lineCharacterIndex)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndexPositionFromLine, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(lineCharacterIndex)));
	}

	/**
	 * Start notifying the container of all key presses and commands.
	 */
	void Editor::StartRecord()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StartRecord, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Stop notifying the container of all key presses and commands.
	 */
	void Editor::StopRecord()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StopRecord, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Colourise a segment of the document using the current lexing language.
	 */
	void Editor::Colourise(int64_t start, int64_t end)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::Colourise, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(end));
	}

	/**
	 * For private communication between an application and a known lexer.
	 */
	uint64_t Editor::PrivateLexerCall(int32_t operation, uint64_t pointer)
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::PrivateLexerCall, static_cast<Scintilla::uptr_t>(operation), static_cast<Scintilla::sptr_t>(pointer)));
	}

	/**
	 * Retrieve a '\n' separated list of properties understood by the current lexer.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::PropertyNamesWriteBuffer(Windows::Storage::Streams::IBuffer const &names)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::PropertyNames, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(names ? names.data() : nullptr)));
	}

	hstring Editor::PropertyNames()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::PropertyNames, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::PropertyNames, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve the type of a property.
	 */
	WinUIEditor::TypeProperty Editor::PropertyTypeFromBuffer(Windows::Storage::Streams::IBuffer const &name)
	{
		return static_cast<WinUIEditor::TypeProperty>(_editor.get()->PublicWndProc(Scintilla::Message::PropertyType, reinterpret_cast<Scintilla::uptr_t>(name ? name.data() : nullptr), static_cast<Scintilla::sptr_t>(0)));
	}

	WinUIEditor::TypeProperty Editor::PropertyType(hstring const &name)
	{
		const auto nameUtf8{ to_string(name) };
		return static_cast<WinUIEditor::TypeProperty>(_editor.get()->PublicWndProc(Scintilla::Message::PropertyType, reinterpret_cast<Scintilla::uptr_t>(nameUtf8.c_str()), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Describe a property.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::DescribePropertyWriteBuffer(Windows::Storage::Streams::IBuffer const &name, Windows::Storage::Streams::IBuffer const &description)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::DescribeProperty, reinterpret_cast<Scintilla::uptr_t>(name ? name.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(description ? description.data() : nullptr)));
	}

	hstring Editor::DescribeProperty(hstring const &name)
	{
		const auto nameUtf8{ to_string(name) };
		const auto wParam{ reinterpret_cast<Scintilla::uptr_t>(nameUtf8.c_str()) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::DescribeProperty, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::DescribeProperty, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve a '\n' separated list of descriptions of the keyword sets understood by the current lexer.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::DescribeKeyWordSetsWriteBuffer(Windows::Storage::Streams::IBuffer const &descriptions)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::DescribeKeyWordSets, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(descriptions ? descriptions.data() : nullptr)));
	}

	hstring Editor::DescribeKeyWordSets()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::DescribeKeyWordSets, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::DescribeKeyWordSets, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Allocate a set of sub styles for a particular base style, returning start of range
	 */
	int32_t Editor::AllocateSubStyles(int32_t styleBase, int32_t numberStyles)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AllocateSubStyles, static_cast<Scintilla::uptr_t>(styleBase), static_cast<Scintilla::sptr_t>(numberStyles)));
	}

	/**
	 * Free allocated sub styles
	 */
	void Editor::FreeSubStyles()
	{
		_editor.get()->PublicWndProc(Scintilla::Message::FreeSubStyles, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Retrieve the name of a style.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::NameOfStyleWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &name)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::NameOfStyle, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(name ? name.data() : nullptr)));
	}

	hstring Editor::NameOfStyle(int32_t style)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(style) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::NameOfStyle, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::NameOfStyle, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve a ' ' separated list of style tags like "literal quoted string".
	 * Result is NUL-terminated.
	 */
	int32_t Editor::TagsOfStyleWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &tags)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::TagsOfStyle, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(tags ? tags.data() : nullptr)));
	}

	hstring Editor::TagsOfStyle(int32_t style)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(style) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::TagsOfStyle, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::TagsOfStyle, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve a description of a style.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::DescriptionOfStyleWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &description)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::DescriptionOfStyle, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(description ? description.data() : nullptr)));
	}

	hstring Editor::DescriptionOfStyle(int32_t style)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(style) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::DescriptionOfStyle, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::DescriptionOfStyle, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Returns the character byte at the position.
	 */
	int32_t Editor::GetCharAt(int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCharAt, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the style byte at the position.
	 */
	int32_t Editor::GetStyleAt(int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetStyleAt, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the unsigned style byte at the position.
	 */
	int32_t Editor::GetStyleIndexAt(int64_t pos)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetStyleIndexAt, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the locale for displaying text.
	 */
	int32_t Editor::GetFontLocaleWriteBuffer(Windows::Storage::Streams::IBuffer const &localeName)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetFontLocale, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(localeName ? localeName.data() : nullptr)));
	}

	hstring Editor::GetFontLocale()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetFontLocale, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetFontLocale, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the layer used for a marker that is drawn in the text area, not the margin.
	 */
	WinUIEditor::Layer Editor::MarkerGetLayer(int32_t markerNumber)
	{
		return static_cast<WinUIEditor::Layer>(_editor.get()->PublicWndProc(Scintilla::Message::MarkerGetLayer, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the type of a margin.
	 */
	WinUIEditor::MarginType Editor::GetMarginTypeN(int32_t margin)
	{
		return static_cast<WinUIEditor::MarginType>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginTypeN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the width of a margin in pixels.
	 */
	int32_t Editor::GetMarginWidthN(int32_t margin)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginWidthN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the marker mask of a margin.
	 */
	int32_t Editor::GetMarginMaskN(int32_t margin)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginMaskN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the mouse click sensitivity of a margin.
	 */
	bool Editor::GetMarginSensitiveN(int32_t margin)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginSensitiveN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the cursor shown in a margin.
	 */
	WinUIEditor::CursorShape Editor::GetMarginCursorN(int32_t margin)
	{
		return static_cast<WinUIEditor::CursorShape>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginCursorN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the background colour of a margin
	 */
	int32_t Editor::GetMarginBackN(int32_t margin)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMarginBackN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the foreground colour of a style.
	 */
	int32_t Editor::StyleGetFore(int32_t style)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetFore, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the background colour of a style.
	 */
	int32_t Editor::StyleGetBack(int32_t style)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetBack, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style bold or not.
	 */
	bool Editor::StyleGetBold(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetBold, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style italic or not.
	 */
	bool Editor::StyleGetItalic(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetItalic, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the size of characters of a style.
	 */
	int32_t Editor::StyleGetSize(int32_t style)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetSize, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the font of a style.
	 * Returns the length of the fontName
	 * Result is NUL-terminated.
	 */
	int32_t Editor::StyleGetFontWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &fontName)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetFont, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(fontName ? fontName.data() : nullptr)));
	}

	hstring Editor::StyleGetFont(int32_t style)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(style) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetFont, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::StyleGetFont, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get is a style to have its end of line filled or not.
	 */
	bool Editor::StyleGetEOLFilled(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetEOLFilled, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style underlined or not.
	 */
	bool Editor::StyleGetUnderline(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetUnderline, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style mixed case, or to force upper or lower case.
	 */
	WinUIEditor::CaseVisible Editor::StyleGetCase(int32_t style)
	{
		return static_cast<WinUIEditor::CaseVisible>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetCase, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the character get of the font in a style.
	 */
	WinUIEditor::CharacterSet Editor::StyleGetCharacterSet(int32_t style)
	{
		return static_cast<WinUIEditor::CharacterSet>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetCharacterSet, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style visible or not.
	 */
	bool Editor::StyleGetVisible(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetVisible, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style changeable or not (read only).
	 * Experimental feature, currently buggy.
	 */
	bool Editor::StyleGetChangeable(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetChangeable, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get is a style a hotspot or not.
	 */
	bool Editor::StyleGetHotSpot(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetHotSpot, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the size of characters of a style in points multiplied by 100
	 */
	int32_t Editor::StyleGetSizeFractional(int32_t style)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetSizeFractional, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the weight of characters of a style.
	 */
	WinUIEditor::FontWeight Editor::StyleGetWeight(int32_t style)
	{
		return static_cast<WinUIEditor::FontWeight>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetWeight, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get whether a style may be monospaced.
	 */
	bool Editor::StyleGetCheckMonospaced(int32_t style)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetCheckMonospaced, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the stretch of characters of a style.
	 */
	WinUIEditor::FontStretch Editor::StyleGetStretch(int32_t style)
	{
		return static_cast<WinUIEditor::FontStretch>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetStretch, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the invisible representation for a style.
	 */
	int32_t Editor::StyleGetInvisibleRepresentationWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &representation)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetInvisibleRepresentation, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(representation ? representation.data() : nullptr)));
	}

	hstring Editor::StyleGetInvisibleRepresentation(int32_t style)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(style) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::StyleGetInvisibleRepresentation, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::StyleGetInvisibleRepresentation, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the colour of an element.
	 */
	int32_t Editor::GetElementColour(WinUIEditor::Element const &element)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetElementColour, static_cast<Scintilla::uptr_t>(element), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get whether an element has been set by SetElementColour.
	 * When false, a platform-defined or default colour is used.
	 */
	bool Editor::GetElementIsSet(WinUIEditor::Element const &element)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetElementIsSet, static_cast<Scintilla::uptr_t>(element), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get whether an element supports translucency.
	 */
	bool Editor::GetElementAllowsTranslucent(WinUIEditor::Element const &element)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetElementAllowsTranslucent, static_cast<Scintilla::uptr_t>(element), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the colour of an element.
	 */
	int32_t Editor::GetElementBaseColour(WinUIEditor::Element const &element)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetElementBaseColour, static_cast<Scintilla::uptr_t>(element), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the set of characters making up words for when moving or selecting by word.
	 * Returns the number of characters
	 */
	int32_t Editor::GetWordCharsWriteBuffer(Windows::Storage::Streams::IBuffer const &characters)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetWordChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characters ? characters.data() : nullptr)));
	}

	hstring Editor::GetWordChars()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetWordChars, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetWordChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * What is the type of an action?
	 */
	int32_t Editor::GetUndoActionType(int32_t action)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoActionType, static_cast<Scintilla::uptr_t>(action), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * What is the position of an action?
	 */
	int64_t Editor::GetUndoActionPosition(int32_t action)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoActionPosition, static_cast<Scintilla::uptr_t>(action), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * What is the text of an action?
	 */
	int32_t Editor::GetUndoActionTextWriteBuffer(int32_t action, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoActionText, static_cast<Scintilla::uptr_t>(action), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetUndoActionText(int32_t action)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(action) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetUndoActionText, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetUndoActionText, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve the style of an indicator.
	 */
	WinUIEditor::IndicatorStyle Editor::IndicGetStyle(int32_t indicator)
	{
		return static_cast<WinUIEditor::IndicatorStyle>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetStyle, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the foreground colour of an indicator.
	 */
	int32_t Editor::IndicGetFore(int32_t indicator)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetFore, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve whether indicator drawn under or over text.
	 */
	bool Editor::IndicGetUnder(int32_t indicator)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetUnder, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the hover style of an indicator.
	 */
	WinUIEditor::IndicatorStyle Editor::IndicGetHoverStyle(int32_t indicator)
	{
		return static_cast<WinUIEditor::IndicatorStyle>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetHoverStyle, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the foreground hover colour of an indicator.
	 */
	int32_t Editor::IndicGetHoverFore(int32_t indicator)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetHoverFore, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the attributes of an indicator.
	 */
	WinUIEditor::IndicFlag Editor::IndicGetFlags(int32_t indicator)
	{
		return static_cast<WinUIEditor::IndicFlag>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetFlags, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the stroke width of an indicator.
	 */
	int32_t Editor::IndicGetStrokeWidth(int32_t indicator)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetStrokeWidth, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the extra styling information for a line.
	 */
	int32_t Editor::GetLineState(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineState, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the number of columns that a line is indented.
	 */
	int32_t Editor::GetLineIndentation(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineIndentation, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the position before the first non indentation character on a line.
	 */
	int64_t Editor::GetLineIndentPosition(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineIndentPosition, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the column number of a position, taking tab width into account.
	 */
	int64_t Editor::GetColumn(int64_t pos)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetColumn, static_cast<Scintilla::uptr_t>(pos), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the position after the last visible characters on a line.
	 */
	int64_t Editor::GetLineEndPosition(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineEndPosition, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve serialized form of selection.
	 */
	int64_t Editor::GetSelectionSerializedWriteBuffer(Windows::Storage::Streams::IBuffer const &selectionString)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionSerialized, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(selectionString ? selectionString.data() : nullptr)));
	}

	hstring Editor::GetSelectionSerialized()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionSerialized, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionSerialized, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve the text in the target.
	 */
	int64_t Editor::GetTargetTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTargetText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::GetTargetText()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTargetText, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetTargetText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve the fold level of a line.
	 */
	WinUIEditor::FoldLevel Editor::GetFoldLevel(int64_t line)
	{
		return static_cast<WinUIEditor::FoldLevel>(_editor.get()->PublicWndProc(Scintilla::Message::GetFoldLevel, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Find the last child line of a header line.
	 */
	int64_t Editor::GetLastChild(int64_t line, WinUIEditor::FoldLevel const &level)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLastChild, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(level)));
	}

	/**
	 * Find the parent line of a child line.
	 */
	int64_t Editor::GetFoldParent(int64_t line)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetFoldParent, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Is a line visible?
	 */
	bool Editor::GetLineVisible(int64_t line)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetLineVisible, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Is a header line expanded?
	 */
	bool Editor::GetFoldExpanded(int64_t line)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::GetFoldExpanded, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve the value of a tag from a regular expression search.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::GetTagWriteBuffer(int32_t tagNumber, Windows::Storage::Streams::IBuffer const &tagValue)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTag, static_cast<Scintilla::uptr_t>(tagNumber), reinterpret_cast<Scintilla::sptr_t>(tagValue ? tagValue.data() : nullptr)));
	}

	hstring Editor::GetTag(int32_t tagNumber)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(tagNumber) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetTag, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetTag, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get multi edge positions.
	 */
	int64_t Editor::GetMultiEdgeColumn(int32_t which)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetMultiEdgeColumn, static_cast<Scintilla::uptr_t>(which), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the fore colour for active hotspots.
	 */
	int32_t Editor::GetHotspotActiveFore()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetHotspotActiveFore, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the back colour for active hotspots.
	 */
	int32_t Editor::GetHotspotActiveBack()
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetHotspotActiveBack, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the set of characters making up whitespace for when moving or selecting by word.
	 */
	int32_t Editor::GetWhitespaceCharsWriteBuffer(Windows::Storage::Streams::IBuffer const &characters)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetWhitespaceChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characters ? characters.data() : nullptr)));
	}

	hstring Editor::GetWhitespaceChars()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetWhitespaceChars, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetWhitespaceChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the set of characters making up punctuation characters
	 */
	int32_t Editor::GetPunctuationCharsWriteBuffer(Windows::Storage::Streams::IBuffer const &characters)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPunctuationChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characters ? characters.data() : nullptr)));
	}

	hstring Editor::GetPunctuationChars()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPunctuationChars, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetPunctuationChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get currently selected item text in the auto-completion list
	 * Returns the length of the item text
	 * Result is NUL-terminated.
	 */
	int32_t Editor::AutoCGetCurrentTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetCurrentText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::AutoCGetCurrentText()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetCurrentText, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::AutoCGetCurrentText, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the string to separate parts when copying a multiple selection.
	 */
	int32_t Editor::GetCopySeparatorWriteBuffer(Windows::Storage::Streams::IBuffer const &separator)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCopySeparator, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(separator ? separator.data() : nullptr)));
	}

	hstring Editor::GetCopySeparator()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetCopySeparator, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetCopySeparator, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Return a read-only pointer to a range of characters in the document.
	 * May move the gap so that the range is contiguous, but will only move up
	 * to lengthRange bytes.
	 */
	uint64_t Editor::GetRangePointer(int64_t start, int64_t lengthRange)
	{
		return static_cast<uint64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRangePointer, static_cast<Scintilla::uptr_t>(start), static_cast<Scintilla::sptr_t>(lengthRange)));
	}

	/**
	 * Get the alpha fill colour of the given indicator.
	 */
	WinUIEditor::Alpha Editor::IndicGetAlpha(int32_t indicator)
	{
		return static_cast<WinUIEditor::Alpha>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetAlpha, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the alpha outline colour of the given indicator.
	 */
	WinUIEditor::Alpha Editor::IndicGetOutlineAlpha(int32_t indicator)
	{
		return static_cast<WinUIEditor::Alpha>(_editor.get()->PublicWndProc(Scintilla::Message::IndicGetOutlineAlpha, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the text in the text margin for a line
	 */
	int32_t Editor::MarginGetTextWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarginGetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::MarginGetText(int64_t line)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(line) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarginGetText, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::MarginGetText, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the style number for the text margin for a line
	 */
	int32_t Editor::MarginGetStyle(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarginGetStyle, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the styles in the text margin for a line
	 */
	int32_t Editor::MarginGetStylesWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarginGetStyles, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(styles ? styles.data() : nullptr)));
	}

	hstring Editor::MarginGetStyles(int64_t line)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(line) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::MarginGetStyles, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::MarginGetStyles, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the annotation text for a line
	 */
	int32_t Editor::AnnotationGetTextWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::AnnotationGetText(int64_t line)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(line) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetText, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetText, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the style number for the annotations for a line
	 */
	int32_t Editor::AnnotationGetStyle(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetStyle, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the annotation styles for a line
	 */
	int32_t Editor::AnnotationGetStylesWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetStyles, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(styles ? styles.data() : nullptr)));
	}

	hstring Editor::AnnotationGetStyles(int64_t line)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(line) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetStyles, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetStyles, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the number of annotation lines for a line
	 */
	int32_t Editor::AnnotationGetLines(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::AnnotationGetLines, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Return the caret position of the nth selection.
	 */
	int64_t Editor::GetSelectionNCaret(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNCaret, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Return the anchor position of the nth selection.
	 */
	int64_t Editor::GetSelectionNAnchor(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNAnchor, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Return the virtual space of the caret of the nth selection.
	 */
	int64_t Editor::GetSelectionNCaretVirtualSpace(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNCaretVirtualSpace, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Return the virtual space of the anchor of the nth selection.
	 */
	int64_t Editor::GetSelectionNAnchorVirtualSpace(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNAnchorVirtualSpace, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the position at the start of the selection.
	 */
	int64_t Editor::GetSelectionNStart(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNStart, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the virtual space at the start of the selection.
	 */
	int64_t Editor::GetSelectionNStartVirtualSpace(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNStartVirtualSpace, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the virtual space at the end of the selection.
	 */
	int64_t Editor::GetSelectionNEndVirtualSpace(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNEndVirtualSpace, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Returns the position at the end of the selection.
	 */
	int64_t Editor::GetSelectionNEnd(int32_t selection)
	{
		return static_cast<int64_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSelectionNEnd, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the way a character is drawn.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::GetRepresentationWriteBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, Windows::Storage::Streams::IBuffer const &representation)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentation, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(representation ? representation.data() : nullptr)));
	}

	hstring Editor::GetRepresentation(hstring const &encodedCharacter)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		const auto wParam{ reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentation, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentation, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the appearance of a representation.
	 */
	WinUIEditor::RepresentationAppearance Editor::GetRepresentationAppearanceFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter)
	{
		return static_cast<WinUIEditor::RepresentationAppearance>(_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentationAppearance, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), static_cast<Scintilla::sptr_t>(0)));
	}

	WinUIEditor::RepresentationAppearance Editor::GetRepresentationAppearance(hstring const &encodedCharacter)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		return static_cast<WinUIEditor::RepresentationAppearance>(_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentationAppearance, reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the colour of a representation.
	 */
	int32_t Editor::GetRepresentationColourFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentationColour, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), static_cast<Scintilla::sptr_t>(0)));
	}

	int32_t Editor::GetRepresentationColour(hstring const &encodedCharacter)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetRepresentationColour, reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the end of line annotation text for a line
	 */
	int32_t Editor::EOLAnnotationGetTextWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationGetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr)));
	}

	hstring Editor::EOLAnnotationGetText(int64_t line)
	{
		const auto wParam{ static_cast<Scintilla::uptr_t>(line) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationGetText, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationGetText, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Get the style number for the end of line annotations for a line
	 */
	int32_t Editor::EOLAnnotationGetStyle(int64_t line)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationGetStyle, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get whether a feature is supported
	 */
	bool Editor::SupportsFeature(WinUIEditor::Supports const &feature)
	{
		return static_cast<bool>(_editor.get()->PublicWndProc(Scintilla::Message::SupportsFeature, static_cast<Scintilla::uptr_t>(feature), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Retrieve a "property" value previously set with SetProperty.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::GetPropertyWriteBuffer(Windows::Storage::Streams::IBuffer const &key, Windows::Storage::Streams::IBuffer const &value)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetProperty, reinterpret_cast<Scintilla::uptr_t>(key ? key.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(value ? value.data() : nullptr)));
	}

	hstring Editor::GetProperty(hstring const &key)
	{
		const auto keyUtf8{ to_string(key) };
		const auto wParam{ reinterpret_cast<Scintilla::uptr_t>(keyUtf8.c_str()) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetProperty, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetProperty, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve a "property" value previously set with SetProperty,
	 * with "$()" variable replacement on returned buffer.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::GetPropertyExpandedWriteBuffer(Windows::Storage::Streams::IBuffer const &key, Windows::Storage::Streams::IBuffer const &value)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPropertyExpanded, reinterpret_cast<Scintilla::uptr_t>(key ? key.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(value ? value.data() : nullptr)));
	}

	hstring Editor::GetPropertyExpanded(hstring const &key)
	{
		const auto keyUtf8{ to_string(key) };
		const auto wParam{ reinterpret_cast<Scintilla::uptr_t>(keyUtf8.c_str()) };
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPropertyExpanded, wParam, static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetPropertyExpanded, wParam, reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Retrieve a "property" value previously set with SetProperty,
	 * interpreted as an int AFTER any "$()" variable replacement.
	 */
	int32_t Editor::GetPropertyIntFromBuffer(Windows::Storage::Streams::IBuffer const &key, int32_t defaultValue)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPropertyInt, reinterpret_cast<Scintilla::uptr_t>(key ? key.data() : nullptr), static_cast<Scintilla::sptr_t>(defaultValue)));
	}

	int32_t Editor::GetPropertyInt(hstring const &key, int32_t defaultValue)
	{
		const auto keyUtf8{ to_string(key) };
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPropertyInt, reinterpret_cast<Scintilla::uptr_t>(keyUtf8.c_str()), static_cast<Scintilla::sptr_t>(defaultValue)));
	}

	/**
	 * Retrieve the name of the lexer.
	 * Return the length of the text.
	 * Result is NUL-terminated.
	 */
	int32_t Editor::GetLexerLanguageWriteBuffer(Windows::Storage::Streams::IBuffer const &language)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLexerLanguage, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(language ? language.data() : nullptr)));
	}

	hstring Editor::GetLexerLanguage()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetLexerLanguage, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetLexerLanguage, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * The starting style number for the sub styles associated with a base style
	 */
	int32_t Editor::GetSubStylesStart(int32_t styleBase)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSubStylesStart, static_cast<Scintilla::uptr_t>(styleBase), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * The number of sub styles associated with a base style
	 */
	int32_t Editor::GetSubStylesLength(int32_t styleBase)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSubStylesLength, static_cast<Scintilla::uptr_t>(styleBase), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * For a sub style, return the base style, else return the argument.
	 */
	int32_t Editor::GetStyleFromSubStyle(int32_t subStyle)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetStyleFromSubStyle, static_cast<Scintilla::uptr_t>(subStyle), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * For a secondary style, return the primary style, else return the argument.
	 */
	int32_t Editor::GetPrimaryStyleFromStyle(int32_t style)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetPrimaryStyleFromStyle, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(0)));
	}

	/**
	 * Get the set of base styles that can be extended with sub styles
	 * Result is NUL-terminated.
	 */
	int32_t Editor::GetSubStyleBasesWriteBuffer(Windows::Storage::Streams::IBuffer const &styles)
	{
		return static_cast<int32_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSubStyleBases, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(styles ? styles.data() : nullptr)));
	}

	hstring Editor::GetSubStyleBases()
	{
		const auto sourceLength{ static_cast<size_t>(_editor.get()->PublicWndProc(Scintilla::Message::GetSubStyleBases, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(0))) };
		const auto len{ sourceLength };
		if (len)
		{
			std::string value(len + 1, '\0');
			_editor.get()->PublicWndProc(Scintilla::Message::GetSubStyleBases, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(value.data()));
			return to_hstring(std::string_view(value.data(), len));
		}
		else
		{
			return hstring{};
		}
	}

	/**
	 * Set the locale for displaying text.
	 */
	void Editor::SetFontLocaleFromBuffer(Windows::Storage::Streams::IBuffer const &localeName)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFontLocale, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(localeName ? localeName.data() : nullptr));
	}

	void Editor::SetFontLocale(hstring const &localeName)
	{
		const auto localeNameUtf8{ to_string(localeName) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetFontLocale, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(localeNameUtf8.c_str()));
	}

	/**
	 * Set the foreground colour used for a particular marker number.
	 */
	void Editor::MarkerSetFore(int32_t markerNumber, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetFore, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set the background colour used for a particular marker number.
	 */
	void Editor::MarkerSetBack(int32_t markerNumber, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetBack, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the background colour used for a particular marker number when its folding block is selected.
	 */
	void Editor::MarkerSetBackSelected(int32_t markerNumber, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetBackSelected, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the foreground colour used for a particular marker number.
	 */
	void Editor::MarkerSetForeTranslucent(int32_t markerNumber, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetForeTranslucent, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set the background colour used for a particular marker number.
	 */
	void Editor::MarkerSetBackTranslucent(int32_t markerNumber, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetBackTranslucent, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the background colour used for a particular marker number when its folding block is selected.
	 */
	void Editor::MarkerSetBackSelectedTranslucent(int32_t markerNumber, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetBackSelectedTranslucent, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the width of strokes used in .01 pixels so 50  = 1/2 pixel width.
	 */
	void Editor::MarkerSetStrokeWidth(int32_t markerNumber, int32_t hundredths)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetStrokeWidth, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(hundredths));
	}

	/**
	 * Set the alpha used for a marker that is drawn in the text area, not the margin.
	 */
	void Editor::MarkerSetAlpha(int32_t markerNumber, WinUIEditor::Alpha const &alpha)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetAlpha, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(alpha));
	}

	/**
	 * Set the layer used for a marker that is drawn in the text area, not the margin.
	 */
	void Editor::MarkerSetLayer(int32_t markerNumber, WinUIEditor::Layer const &layer)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarkerSetLayer, static_cast<Scintilla::uptr_t>(markerNumber), static_cast<Scintilla::sptr_t>(layer));
	}

	/**
	 * Set a margin to be either numeric or symbolic.
	 */
	void Editor::SetMarginTypeN(int32_t margin, WinUIEditor::MarginType const &marginType)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginTypeN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(marginType));
	}

	/**
	 * Set the width of a margin to a width expressed in pixels.
	 */
	void Editor::SetMarginWidthN(int32_t margin, int32_t pixelWidth)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginWidthN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(pixelWidth));
	}

	/**
	 * Set a mask that determines which markers are displayed in a margin.
	 */
	void Editor::SetMarginMaskN(int32_t margin, int32_t mask)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginMaskN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(mask));
	}

	/**
	 * Make a margin sensitive or insensitive to mouse clicks.
	 */
	void Editor::SetMarginSensitiveN(int32_t margin, bool sensitive)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginSensitiveN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(sensitive));
	}

	/**
	 * Set the cursor shown when the mouse is inside a margin.
	 */
	void Editor::SetMarginCursorN(int32_t margin, WinUIEditor::CursorShape const &cursor)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginCursorN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(cursor));
	}

	/**
	 * Set the background colour of a margin. Only visible for SC_MARGIN_COLOUR.
	 */
	void Editor::SetMarginBackN(int32_t margin, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetMarginBackN, static_cast<Scintilla::uptr_t>(margin), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the foreground colour of a style.
	 */
	void Editor::StyleSetFore(int32_t style, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetFore, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set the background colour of a style.
	 */
	void Editor::StyleSetBack(int32_t style, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetBack, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set a style to be bold or not.
	 */
	void Editor::StyleSetBold(int32_t style, bool bold)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetBold, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(bold));
	}

	/**
	 * Set a style to be italic or not.
	 */
	void Editor::StyleSetItalic(int32_t style, bool italic)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetItalic, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(italic));
	}

	/**
	 * Set the size of characters of a style.
	 */
	void Editor::StyleSetSize(int32_t style, int32_t sizePoints)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetSize, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(sizePoints));
	}

	/**
	 * Set the font of a style.
	 */
	void Editor::StyleSetFontFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &fontName)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetFont, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(fontName ? fontName.data() : nullptr));
	}

	void Editor::StyleSetFont(int32_t style, hstring const &fontName)
	{
		const auto fontNameUtf8{ to_string(fontName) };
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetFont, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(fontNameUtf8.c_str()));
	}

	/**
	 * Set a style to have its end of line filled or not.
	 */
	void Editor::StyleSetEOLFilled(int32_t style, bool eolFilled)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetEOLFilled, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(eolFilled));
	}

	/**
	 * Set a style to be underlined or not.
	 */
	void Editor::StyleSetUnderline(int32_t style, bool underline)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetUnderline, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(underline));
	}

	/**
	 * Set a style to be mixed case, or to force upper or lower case.
	 */
	void Editor::StyleSetCase(int32_t style, WinUIEditor::CaseVisible const &caseVisible)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetCase, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(caseVisible));
	}

	/**
	 * Set the size of characters of a style. Size is in points multiplied by 100.
	 */
	void Editor::StyleSetSizeFractional(int32_t style, int32_t sizeHundredthPoints)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetSizeFractional, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(sizeHundredthPoints));
	}

	/**
	 * Set the weight of characters of a style.
	 */
	void Editor::StyleSetWeight(int32_t style, WinUIEditor::FontWeight const &weight)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetWeight, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(weight));
	}

	/**
	 * Set the character set of the font in a style.
	 */
	void Editor::StyleSetCharacterSet(int32_t style, WinUIEditor::CharacterSet const &characterSet)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetCharacterSet, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(characterSet));
	}

	/**
	 * Set a style to be a hotspot or not.
	 */
	void Editor::StyleSetHotSpot(int32_t style, bool hotspot)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetHotSpot, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(hotspot));
	}

	/**
	 * Indicate that a style may be monospaced over ASCII graphics characters which enables optimizations.
	 */
	void Editor::StyleSetCheckMonospaced(int32_t style, bool checkMonospaced)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetCheckMonospaced, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(checkMonospaced));
	}

	/**
	 * Set the stretch of characters of a style.
	 */
	void Editor::StyleSetStretch(int32_t style, WinUIEditor::FontStretch const &stretch)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetStretch, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(stretch));
	}

	/**
	 * Set the invisible representation for a style.
	 */
	void Editor::StyleSetInvisibleRepresentationFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &representation)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetInvisibleRepresentation, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(representation ? representation.data() : nullptr));
	}

	void Editor::StyleSetInvisibleRepresentation(int32_t style, hstring const &representation)
	{
		const auto representationUtf8{ to_string(representation) };
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetInvisibleRepresentation, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(representationUtf8.c_str()));
	}

	/**
	 * Set the colour of an element. Translucency (alpha) may or may not be significant
	 * and this may depend on the platform. The alpha byte should commonly be 0xff for opaque.
	 */
	void Editor::SetElementColour(WinUIEditor::Element const &element, int32_t colourElement)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetElementColour, static_cast<Scintilla::uptr_t>(element), static_cast<Scintilla::sptr_t>(colourElement));
	}

	/**
	 * Set a style to be visible or not.
	 */
	void Editor::StyleSetVisible(int32_t style, bool visible)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetVisible, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(visible));
	}

	/**
	 * Set the set of characters making up words for when moving or selecting by word.
	 * First sets defaults like SetCharsDefault.
	 */
	void Editor::SetWordCharsFromBuffer(Windows::Storage::Streams::IBuffer const &characters)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWordChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characters ? characters.data() : nullptr));
	}

	void Editor::SetWordChars(hstring const &characters)
	{
		const auto charactersUtf8{ to_string(characters) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetWordChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(charactersUtf8.c_str()));
	}

	/**
	 * Set an indicator to plain, squiggle or TT.
	 */
	void Editor::IndicSetStyle(int32_t indicator, WinUIEditor::IndicatorStyle const &indicatorStyle)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetStyle, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(indicatorStyle));
	}

	/**
	 * Set the foreground colour of an indicator.
	 */
	void Editor::IndicSetFore(int32_t indicator, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetFore, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set an indicator to draw under text or over(default).
	 */
	void Editor::IndicSetUnder(int32_t indicator, bool under)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetUnder, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(under));
	}

	/**
	 * Set a hover indicator to plain, squiggle or TT.
	 */
	void Editor::IndicSetHoverStyle(int32_t indicator, WinUIEditor::IndicatorStyle const &indicatorStyle)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetHoverStyle, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(indicatorStyle));
	}

	/**
	 * Set the foreground hover colour of an indicator.
	 */
	void Editor::IndicSetHoverFore(int32_t indicator, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetHoverFore, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set the attributes of an indicator.
	 */
	void Editor::IndicSetFlags(int32_t indicator, WinUIEditor::IndicFlag const &flags)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetFlags, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(flags));
	}

	/**
	 * Set the stroke width of an indicator in hundredths of a pixel.
	 */
	void Editor::IndicSetStrokeWidth(int32_t indicator, int32_t hundredths)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetStrokeWidth, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(hundredths));
	}

	/**
	 * Used to hold extra styling information for each line.
	 */
	void Editor::SetLineState(int64_t line, int32_t state)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetLineState, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(state));
	}

	/**
	 * Set a style to be changeable or not (read only).
	 * Experimental feature, currently buggy.
	 */
	void Editor::StyleSetChangeable(int32_t style, bool changeable)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::StyleSetChangeable, static_cast<Scintilla::uptr_t>(style), static_cast<Scintilla::sptr_t>(changeable));
	}

	/**
	 * Define a set of characters that when typed will cause the autocompletion to
	 * choose the selected item.
	 */
	void Editor::AutoCSetFillUpsFromBuffer(Windows::Storage::Streams::IBuffer const &characterSet)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetFillUps, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characterSet ? characterSet.data() : nullptr));
	}

	void Editor::AutoCSetFillUps(hstring const &characterSet)
	{
		const auto characterSetUtf8{ to_string(characterSet) };
		_editor.get()->PublicWndProc(Scintilla::Message::AutoCSetFillUps, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characterSetUtf8.c_str()));
	}

	/**
	 * Change the indentation of a line to a number of columns.
	 */
	void Editor::SetLineIndentation(int64_t line, int32_t indentation)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetLineIndentation, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(indentation));
	}

	/**
	 * Set selection from serialized form.
	 */
	void Editor::SetSelectionSerializedFromBuffer(Windows::Storage::Streams::IBuffer const &selectionString)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionSerialized, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(selectionString ? selectionString.data() : nullptr));
	}

	void Editor::SetSelectionSerialized(hstring const &selectionString)
	{
		const auto selectionStringUtf8{ to_string(selectionString) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionSerialized, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(selectionStringUtf8.c_str()));
	}

	/**
	 * Enlarge the number of lines allocated.
	 */
	void Editor::AllocateLines(int64_t lines)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AllocateLines, static_cast<Scintilla::uptr_t>(lines), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the start position in order to change when backspacing removes the calltip.
	 */
	void Editor::CallTipSetPosStart(int64_t posStart)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipSetPosStart, static_cast<Scintilla::uptr_t>(posStart), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the background colour for the call tip.
	 */
	void Editor::CallTipSetBack(int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipSetBack, static_cast<Scintilla::uptr_t>(back), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the foreground colour for the call tip.
	 */
	void Editor::CallTipSetFore(int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipSetFore, static_cast<Scintilla::uptr_t>(fore), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the foreground colour for the highlighted part of the call tip.
	 */
	void Editor::CallTipSetForeHlt(int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipSetForeHlt, static_cast<Scintilla::uptr_t>(fore), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Enable use of STYLE_CALLTIP and set call tip tab size in pixels.
	 */
	void Editor::CallTipUseStyle(int32_t tabSize)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipUseStyle, static_cast<Scintilla::uptr_t>(tabSize), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set position of calltip, above or below text.
	 */
	void Editor::CallTipSetPosition(bool above)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::CallTipSetPosition, static_cast<Scintilla::uptr_t>(above), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the fold level of a line.
	 * This encodes an integer level along with flags indicating whether the
	 * line is a header and whether it is effectively white space.
	 */
	void Editor::SetFoldLevel(int64_t line, WinUIEditor::FoldLevel const &level)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFoldLevel, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(level));
	}

	/**
	 * Show the children of a header line.
	 */
	void Editor::SetFoldExpanded(int64_t line, bool expanded)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFoldExpanded, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(expanded));
	}

	/**
	 * Set some style options for folding.
	 */
	void Editor::SetFoldFlags(WinUIEditor::FoldFlag const &flags)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetFoldFlags, static_cast<Scintilla::uptr_t>(flags), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set a fore colour for active hotspots.
	 */
	void Editor::SetHotspotActiveFore(bool useSetting, int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetHotspotActiveFore, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(fore));
	}

	/**
	 * Set a back colour for active hotspots.
	 */
	void Editor::SetHotspotActiveBack(bool useSetting, int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetHotspotActiveBack, static_cast<Scintilla::uptr_t>(useSetting), static_cast<Scintilla::sptr_t>(back));
	}

	/**
	 * Set the set of characters making up whitespace for when moving or selecting by word.
	 * Should be called after SetWordChars.
	 */
	void Editor::SetWhitespaceCharsFromBuffer(Windows::Storage::Streams::IBuffer const &characters)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetWhitespaceChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characters ? characters.data() : nullptr));
	}

	void Editor::SetWhitespaceChars(hstring const &characters)
	{
		const auto charactersUtf8{ to_string(characters) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetWhitespaceChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(charactersUtf8.c_str()));
	}

	/**
	 * Set the set of characters making up punctuation characters
	 * Should be called after SetWordChars.
	 */
	void Editor::SetPunctuationCharsFromBuffer(Windows::Storage::Streams::IBuffer const &characters)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetPunctuationChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(characters ? characters.data() : nullptr));
	}

	void Editor::SetPunctuationChars(hstring const &characters)
	{
		const auto charactersUtf8{ to_string(characters) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetPunctuationChars, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(charactersUtf8.c_str()));
	}

	/**
	 * Set the string to separate parts when copying a multiple selection.
	 */
	void Editor::SetCopySeparatorFromBuffer(Windows::Storage::Streams::IBuffer const &separator)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetCopySeparator, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(separator ? separator.data() : nullptr));
	}

	void Editor::SetCopySeparator(hstring const &separator)
	{
		const auto separatorUtf8{ to_string(separator) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetCopySeparator, static_cast<Scintilla::uptr_t>(0), reinterpret_cast<Scintilla::sptr_t>(separatorUtf8.c_str()));
	}

	/**
	 * Set the alpha fill colour of the given indicator.
	 */
	void Editor::IndicSetAlpha(int32_t indicator, WinUIEditor::Alpha const &alpha)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetAlpha, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(alpha));
	}

	/**
	 * Set the alpha outline colour of the given indicator.
	 */
	void Editor::IndicSetOutlineAlpha(int32_t indicator, WinUIEditor::Alpha const &alpha)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::IndicSetOutlineAlpha, static_cast<Scintilla::uptr_t>(indicator), static_cast<Scintilla::sptr_t>(alpha));
	}

	/**
	 * Set the text in the text margin for a line
	 */
	void Editor::MarginSetTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarginSetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::MarginSetText(int64_t line, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::MarginSetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Set the style number for the text margin for a line
	 */
	void Editor::MarginSetStyle(int64_t line, int32_t style)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarginSetStyle, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(style));
	}

	/**
	 * Set the style in the text margin for a line
	 */
	void Editor::MarginSetStylesFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::MarginSetStyles, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(styles ? styles.data() : nullptr));
	}

	void Editor::MarginSetStyles(int64_t line, hstring const &styles)
	{
		const auto stylesUtf8{ to_string(styles) };
		_editor.get()->PublicWndProc(Scintilla::Message::MarginSetStyles, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(stylesUtf8.c_str()));
	}

	/**
	 * Set the annotation text for a line
	 */
	void Editor::AnnotationSetTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::AnnotationSetText(int64_t line, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Set the style number for the annotations for a line
	 */
	void Editor::AnnotationSetStyle(int64_t line, int32_t style)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetStyle, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(style));
	}

	/**
	 * Set the annotation styles for a line
	 */
	void Editor::AnnotationSetStylesFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetStyles, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(styles ? styles.data() : nullptr));
	}

	void Editor::AnnotationSetStyles(int64_t line, hstring const &styles)
	{
		const auto stylesUtf8{ to_string(styles) };
		_editor.get()->PublicWndProc(Scintilla::Message::AnnotationSetStyles, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(stylesUtf8.c_str()));
	}

	/**
	 * Set the caret position of the nth selection.
	 */
	void Editor::SetSelectionNCaret(int32_t selection, int64_t caret)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionNCaret, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(caret));
	}

	/**
	 * Set the anchor position of the nth selection.
	 */
	void Editor::SetSelectionNAnchor(int32_t selection, int64_t anchor)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionNAnchor, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(anchor));
	}

	/**
	 * Set the virtual space of the caret of the nth selection.
	 */
	void Editor::SetSelectionNCaretVirtualSpace(int32_t selection, int64_t space)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionNCaretVirtualSpace, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(space));
	}

	/**
	 * Set the virtual space of the anchor of the nth selection.
	 */
	void Editor::SetSelectionNAnchorVirtualSpace(int32_t selection, int64_t space)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionNAnchorVirtualSpace, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(space));
	}

	/**
	 * Sets the position that starts the selection - this becomes the anchor.
	 */
	void Editor::SetSelectionNStart(int32_t selection, int64_t anchor)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionNStart, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(anchor));
	}

	/**
	 * Sets the position that ends the selection - this becomes the currentPosition.
	 */
	void Editor::SetSelectionNEnd(int32_t selection, int64_t caret)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetSelectionNEnd, static_cast<Scintilla::uptr_t>(selection), static_cast<Scintilla::sptr_t>(caret));
	}

	/**
	 * Set the foreground colour of additional selections.
	 * Must have previously called SetSelFore with non-zero first argument for this to have an effect.
	 */
	void Editor::SetAdditionalSelFore(int32_t fore)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalSelFore, static_cast<Scintilla::uptr_t>(fore), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the background colour of additional selections.
	 * Must have previously called SetSelBack with non-zero first argument for this to have an effect.
	 */
	void Editor::SetAdditionalSelBack(int32_t back)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetAdditionalSelBack, static_cast<Scintilla::uptr_t>(back), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the width for future RGBA image data.
	 */
	void Editor::RGBAImageSetWidth(int32_t width)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::RGBAImageSetWidth, static_cast<Scintilla::uptr_t>(width), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the height for future RGBA image data.
	 */
	void Editor::RGBAImageSetHeight(int32_t height)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::RGBAImageSetHeight, static_cast<Scintilla::uptr_t>(height), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the scale factor in percent for future RGBA image data.
	 */
	void Editor::RGBAImageSetScale(int32_t scalePercent)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::RGBAImageSetScale, static_cast<Scintilla::uptr_t>(scalePercent), static_cast<Scintilla::sptr_t>(0));
	}

	/**
	 * Set the way a character is drawn.
	 */
	void Editor::SetRepresentationFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, Windows::Storage::Streams::IBuffer const &representation)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRepresentation, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(representation ? representation.data() : nullptr));
	}

	void Editor::SetRepresentation(hstring const &encodedCharacter, hstring const &representation)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		const auto representationUtf8{ to_string(representation) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetRepresentation, reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()), reinterpret_cast<Scintilla::sptr_t>(representationUtf8.c_str()));
	}

	/**
	 * Set the appearance of a representation.
	 */
	void Editor::SetRepresentationAppearanceFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, WinUIEditor::RepresentationAppearance const &appearance)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRepresentationAppearance, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), static_cast<Scintilla::sptr_t>(appearance));
	}

	void Editor::SetRepresentationAppearance(hstring const &encodedCharacter, WinUIEditor::RepresentationAppearance const &appearance)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetRepresentationAppearance, reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()), static_cast<Scintilla::sptr_t>(appearance));
	}

	/**
	 * Set the colour of a representation.
	 */
	void Editor::SetRepresentationColourFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, int32_t colour)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetRepresentationColour, reinterpret_cast<Scintilla::uptr_t>(encodedCharacter ? encodedCharacter.data() : nullptr), static_cast<Scintilla::sptr_t>(colour));
	}

	void Editor::SetRepresentationColour(hstring const &encodedCharacter, int32_t colour)
	{
		const auto encodedCharacterUtf8{ to_string(encodedCharacter) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetRepresentationColour, reinterpret_cast<Scintilla::uptr_t>(encodedCharacterUtf8.c_str()), static_cast<Scintilla::sptr_t>(colour));
	}

	/**
	 * Set the end of line annotation text for a line
	 */
	void Editor::EOLAnnotationSetTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationSetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(text ? text.data() : nullptr));
	}

	void Editor::EOLAnnotationSetText(int64_t line, hstring const &text)
	{
		const auto textUtf8{ to_string(text) };
		_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationSetText, static_cast<Scintilla::uptr_t>(line), reinterpret_cast<Scintilla::sptr_t>(textUtf8.c_str()));
	}

	/**
	 * Set the style number for the end of line annotations for a line
	 */
	void Editor::EOLAnnotationSetStyle(int64_t line, int32_t style)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::EOLAnnotationSetStyle, static_cast<Scintilla::uptr_t>(line), static_cast<Scintilla::sptr_t>(style));
	}

	/**
	 * Set up a value that may be used by a lexer for some optional feature.
	 */
	void Editor::SetPropertyFromBuffer(Windows::Storage::Streams::IBuffer const &key, Windows::Storage::Streams::IBuffer const &value)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetProperty, reinterpret_cast<Scintilla::uptr_t>(key ? key.data() : nullptr), reinterpret_cast<Scintilla::sptr_t>(value ? value.data() : nullptr));
	}

	void Editor::SetProperty(hstring const &key, hstring const &value)
	{
		const auto keyUtf8{ to_string(key) };
		const auto valueUtf8{ to_string(value) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetProperty, reinterpret_cast<Scintilla::uptr_t>(keyUtf8.c_str()), reinterpret_cast<Scintilla::sptr_t>(valueUtf8.c_str()));
	}

	/**
	 * Set up the key words used by the lexer.
	 */
	void Editor::SetKeyWordsFromBuffer(int32_t keyWordSet, Windows::Storage::Streams::IBuffer const &keyWords)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetKeyWords, static_cast<Scintilla::uptr_t>(keyWordSet), reinterpret_cast<Scintilla::sptr_t>(keyWords ? keyWords.data() : nullptr));
	}

	void Editor::SetKeyWords(int32_t keyWordSet, hstring const &keyWords)
	{
		const auto keyWordsUtf8{ to_string(keyWords) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetKeyWords, static_cast<Scintilla::uptr_t>(keyWordSet), reinterpret_cast<Scintilla::sptr_t>(keyWordsUtf8.c_str()));
	}

	/**
	 * Set the identifiers that are shown in a particular style
	 */
	void Editor::SetIdentifiersFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &identifiers)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetIdentifiers, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(identifiers ? identifiers.data() : nullptr));
	}

	void Editor::SetIdentifiers(int32_t style, hstring const &identifiers)
	{
		const auto identifiersUtf8{ to_string(identifiers) };
		_editor.get()->PublicWndProc(Scintilla::Message::SetIdentifiers, static_cast<Scintilla::uptr_t>(style), reinterpret_cast<Scintilla::sptr_t>(identifiersUtf8.c_str()));
	}

	/**
	 * Set the lexer from an ILexer*.
	 */
	void Editor::SetILexer(uint64_t ilexer)
	{
		_editor.get()->PublicWndProc(Scintilla::Message::SetILexer, static_cast<Scintilla::uptr_t>(0), static_cast<Scintilla::sptr_t>(ilexer));
	}
}
