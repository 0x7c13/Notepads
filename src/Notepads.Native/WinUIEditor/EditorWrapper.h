// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once
#include "Editor.g.h"
#include "EditorBaseControl.h"
#include "NativeRegex.h"
#include "NativeDiff.h"
#include "StyleNeededEventArgs.g.h"
#include "CharAddedEventArgs.g.h"
#include "SavePointReachedEventArgs.g.h"
#include "SavePointLeftEventArgs.g.h"
#include "ModifyAttemptROEventArgs.g.h"
#include "KeyEventArgs.g.h"
#include "DoubleClickEventArgs.g.h"
#include "UpdateUIEventArgs.g.h"
#include "ModifiedEventArgs.g.h"
#include "MacroRecordEventArgs.g.h"
#include "MarginClickEventArgs.g.h"
#include "NeedShownEventArgs.g.h"
#include "PaintedEventArgs.g.h"
#include "UserListSelectionEventArgs.g.h"
#include "URIDroppedEventArgs.g.h"
#include "DwellStartEventArgs.g.h"
#include "DwellEndEventArgs.g.h"
#include "ZoomChangedEventArgs.g.h"
#include "HotSpotClickEventArgs.g.h"
#include "HotSpotDoubleClickEventArgs.g.h"
#include "CallTipClickEventArgs.g.h"
#include "AutoCSelectionEventArgs.g.h"
#include "IndicatorClickEventArgs.g.h"
#include "IndicatorReleaseEventArgs.g.h"
#include "AutoCCancelledEventArgs.g.h"
#include "AutoCCharDeletedEventArgs.g.h"
#include "HotSpotReleaseClickEventArgs.g.h"
#include "FocusInEventArgs.g.h"
#include "FocusOutEventArgs.g.h"
#include "AutoCCompletedEventArgs.g.h"
#include "MarginRightClickEventArgs.g.h"
#include "AutoCSelectionChangeEventArgs.g.h"

namespace winrt::WinUIEditor::implementation
{
	struct StyleNeededEventArgs : StyleNeededEventArgsT<StyleNeededEventArgs>
	{
		StyleNeededEventArgs(int32_t position);

		int32_t Position();

	private:
		int32_t _position;
	};

	struct CharAddedEventArgs : CharAddedEventArgsT<CharAddedEventArgs>
	{
		CharAddedEventArgs(int32_t ch, int32_t characterSource);

		int32_t Ch();
		int32_t CharacterSource();

	private:
		int32_t _ch;
		int32_t _characterSource;
	};

	struct SavePointReachedEventArgs : SavePointReachedEventArgsT<SavePointReachedEventArgs>
	{
	};

	struct SavePointLeftEventArgs : SavePointLeftEventArgsT<SavePointLeftEventArgs>
	{
	};

	struct ModifyAttemptROEventArgs : ModifyAttemptROEventArgsT<ModifyAttemptROEventArgs>
	{
	};

	struct KeyEventArgs : KeyEventArgsT<KeyEventArgs>
	{
		KeyEventArgs(int32_t ch, int32_t modifiers);

		int32_t Ch();
		int32_t Modifiers();

	private:
		int32_t _ch;
		int32_t _modifiers;
	};

	struct DoubleClickEventArgs : DoubleClickEventArgsT<DoubleClickEventArgs>
	{
		DoubleClickEventArgs(int32_t modifiers, int32_t position, int32_t line);

		int32_t Modifiers();
		int32_t Position();
		int32_t Line();

	private:
		int32_t _modifiers;
		int32_t _position;
		int32_t _line;
	};

	struct UpdateUIEventArgs : UpdateUIEventArgsT<UpdateUIEventArgs>
	{
		UpdateUIEventArgs(int32_t updated);

		int32_t Updated();

	private:
		int32_t _updated;
	};

	struct ModifiedEventArgs : ModifiedEventArgsT<ModifiedEventArgs>
	{
		ModifiedEventArgs(int32_t position, int32_t modificationType, const char *text, int32_t length, int32_t linesAdded, int32_t line, int32_t foldLevelNow, int32_t foldLevelPrev, int32_t token, int32_t annotationLinesAdded);

		void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }
		int32_t Position();
		int32_t ModificationType();
		Windows::Storage::Streams::IBuffer TextAsBuffer();
		hstring Text();
		int32_t Length();
		int32_t LinesAdded();
		int32_t Line();
		int32_t FoldLevelNow();
		int32_t FoldLevelPrev();
		int32_t Token();
		int32_t AnnotationLinesAdded();

	private:
		std::mutex _textMutex;
		bool _textExpired{};
		int32_t _position;
		int32_t _modificationType;
		Windows::Storage::Streams::IBuffer _textAsBuffer;
		Windows::Foundation::IReference<hstring> _text;
		const char *_textAsPointer;
		int32_t _length;
		int32_t _linesAdded;
		int32_t _line;
		int32_t _foldLevelNow;
		int32_t _foldLevelPrev;
		int32_t _token;
		int32_t _annotationLinesAdded;
	};

	struct MacroRecordEventArgs : MacroRecordEventArgsT<MacroRecordEventArgs>
	{
		MacroRecordEventArgs(int32_t message, int32_t wParam, int32_t lParam);

		int32_t Message();
		int32_t WParam();
		int32_t LParam();

	private:
		int32_t _message;
		int32_t _wParam;
		int32_t _lParam;
	};

	struct MarginClickEventArgs : MarginClickEventArgsT<MarginClickEventArgs>
	{
		MarginClickEventArgs(int32_t modifiers, int32_t position, int32_t margin);

		int32_t Modifiers();
		int32_t Position();
		int32_t Margin();

	private:
		int32_t _modifiers;
		int32_t _position;
		int32_t _margin;
	};

	struct NeedShownEventArgs : NeedShownEventArgsT<NeedShownEventArgs>
	{
		NeedShownEventArgs(int32_t position, int32_t length);

		int32_t Position();
		int32_t Length();

	private:
		int32_t _position;
		int32_t _length;
	};

	struct PaintedEventArgs : PaintedEventArgsT<PaintedEventArgs>
	{
	};

	struct UserListSelectionEventArgs : UserListSelectionEventArgsT<UserListSelectionEventArgs>
	{
		UserListSelectionEventArgs(int32_t listType, const char *text, int32_t position, int32_t ch, WinUIEditor::CompletionMethods const &listCompletionMethod);

		void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }
		int32_t ListType();
		Windows::Storage::Streams::IBuffer TextAsBuffer();
		hstring Text();
		int32_t Position();
		int32_t Ch();
		WinUIEditor::CompletionMethods ListCompletionMethod();

	private:
		std::mutex _textMutex;
		bool _textExpired{};
		int32_t _listType;
		Windows::Storage::Streams::IBuffer _textAsBuffer;
		Windows::Foundation::IReference<hstring> _text;
		const char *_textAsPointer;
		int32_t _position;
		int32_t _ch;
		WinUIEditor::CompletionMethods _listCompletionMethod;
	};

	struct URIDroppedEventArgs : URIDroppedEventArgsT<URIDroppedEventArgs>
	{
		URIDroppedEventArgs(const char *text);

		void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }
		Windows::Storage::Streams::IBuffer TextAsBuffer();
		hstring Text();

	private:
		std::mutex _textMutex;
		bool _textExpired{};
		Windows::Storage::Streams::IBuffer _textAsBuffer;
		Windows::Foundation::IReference<hstring> _text;
		const char *_textAsPointer;
	};

	struct DwellStartEventArgs : DwellStartEventArgsT<DwellStartEventArgs>
	{
		DwellStartEventArgs(int32_t position, int32_t x, int32_t y);

		int32_t Position();
		int32_t X();
		int32_t Y();

	private:
		int32_t _position;
		int32_t _x;
		int32_t _y;
	};

	struct DwellEndEventArgs : DwellEndEventArgsT<DwellEndEventArgs>
	{
		DwellEndEventArgs(int32_t position, int32_t x, int32_t y);

		int32_t Position();
		int32_t X();
		int32_t Y();

	private:
		int32_t _position;
		int32_t _x;
		int32_t _y;
	};

	struct ZoomChangedEventArgs : ZoomChangedEventArgsT<ZoomChangedEventArgs>
	{
	};

	struct HotSpotClickEventArgs : HotSpotClickEventArgsT<HotSpotClickEventArgs>
	{
		HotSpotClickEventArgs(int32_t modifiers, int32_t position);

		int32_t Modifiers();
		int32_t Position();

	private:
		int32_t _modifiers;
		int32_t _position;
	};

	struct HotSpotDoubleClickEventArgs : HotSpotDoubleClickEventArgsT<HotSpotDoubleClickEventArgs>
	{
		HotSpotDoubleClickEventArgs(int32_t modifiers, int32_t position);

		int32_t Modifiers();
		int32_t Position();

	private:
		int32_t _modifiers;
		int32_t _position;
	};

	struct CallTipClickEventArgs : CallTipClickEventArgsT<CallTipClickEventArgs>
	{
		CallTipClickEventArgs(int32_t position);

		int32_t Position();

	private:
		int32_t _position;
	};

	struct AutoCSelectionEventArgs : AutoCSelectionEventArgsT<AutoCSelectionEventArgs>
	{
		AutoCSelectionEventArgs(const char *text, int32_t position, int32_t ch, WinUIEditor::CompletionMethods const &listCompletionMethod);

		void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }
		Windows::Storage::Streams::IBuffer TextAsBuffer();
		hstring Text();
		int32_t Position();
		int32_t Ch();
		WinUIEditor::CompletionMethods ListCompletionMethod();

	private:
		std::mutex _textMutex;
		bool _textExpired{};
		Windows::Storage::Streams::IBuffer _textAsBuffer;
		Windows::Foundation::IReference<hstring> _text;
		const char *_textAsPointer;
		int32_t _position;
		int32_t _ch;
		WinUIEditor::CompletionMethods _listCompletionMethod;
	};

	struct IndicatorClickEventArgs : IndicatorClickEventArgsT<IndicatorClickEventArgs>
	{
		IndicatorClickEventArgs(int32_t modifiers, int32_t position);

		int32_t Modifiers();
		int32_t Position();

	private:
		int32_t _modifiers;
		int32_t _position;
	};

	struct IndicatorReleaseEventArgs : IndicatorReleaseEventArgsT<IndicatorReleaseEventArgs>
	{
		IndicatorReleaseEventArgs(int32_t modifiers, int32_t position);

		int32_t Modifiers();
		int32_t Position();

	private:
		int32_t _modifiers;
		int32_t _position;
	};

	struct AutoCCancelledEventArgs : AutoCCancelledEventArgsT<AutoCCancelledEventArgs>
	{
	};

	struct AutoCCharDeletedEventArgs : AutoCCharDeletedEventArgsT<AutoCCharDeletedEventArgs>
	{
	};

	struct HotSpotReleaseClickEventArgs : HotSpotReleaseClickEventArgsT<HotSpotReleaseClickEventArgs>
	{
		HotSpotReleaseClickEventArgs(int32_t modifiers, int32_t position);

		int32_t Modifiers();
		int32_t Position();

	private:
		int32_t _modifiers;
		int32_t _position;
	};

	struct FocusInEventArgs : FocusInEventArgsT<FocusInEventArgs>
	{
	};

	struct FocusOutEventArgs : FocusOutEventArgsT<FocusOutEventArgs>
	{
	};

	struct AutoCCompletedEventArgs : AutoCCompletedEventArgsT<AutoCCompletedEventArgs>
	{
		AutoCCompletedEventArgs(const char *text, int32_t position, int32_t ch, WinUIEditor::CompletionMethods const &listCompletionMethod);

		void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }
		Windows::Storage::Streams::IBuffer TextAsBuffer();
		hstring Text();
		int32_t Position();
		int32_t Ch();
		WinUIEditor::CompletionMethods ListCompletionMethod();

	private:
		std::mutex _textMutex;
		bool _textExpired{};
		Windows::Storage::Streams::IBuffer _textAsBuffer;
		Windows::Foundation::IReference<hstring> _text;
		const char *_textAsPointer;
		int32_t _position;
		int32_t _ch;
		WinUIEditor::CompletionMethods _listCompletionMethod;
	};

	struct MarginRightClickEventArgs : MarginRightClickEventArgsT<MarginRightClickEventArgs>
	{
		MarginRightClickEventArgs(int32_t modifiers, int32_t position, int32_t margin);

		int32_t Modifiers();
		int32_t Position();
		int32_t Margin();

	private:
		int32_t _modifiers;
		int32_t _position;
		int32_t _margin;
	};

	struct AutoCSelectionChangeEventArgs : AutoCSelectionChangeEventArgsT<AutoCSelectionChangeEventArgs>
	{
		AutoCSelectionChangeEventArgs(int32_t listType, const char *text, int32_t position);

		void InvalidateText() noexcept { std::lock_guard guard(_textMutex); _textExpired = true; _textAsPointer = nullptr; }
		int32_t ListType();
		Windows::Storage::Streams::IBuffer TextAsBuffer();
		hstring Text();
		int32_t Position();

	private:
		std::mutex _textMutex;
		bool _textExpired{};
		int32_t _listType;
		Windows::Storage::Streams::IBuffer _textAsBuffer;
		Windows::Foundation::IReference<hstring> _text;
		const char *_textAsPointer;
		int32_t _position;
	};

	struct Editor : EditorT<Editor>
	{
#include "EditorExtensions.h"

		Editor(com_ptr<EditorBaseControl> const &editor);

		void ProcessEvent(Scintilla::NotificationData *data);

		event_token StyleNeeded(WinUIEditor::StyleNeededHandler const &handler);
		void StyleNeeded(event_token const &token);

		event_token CharAdded(WinUIEditor::CharAddedHandler const &handler);
		void CharAdded(event_token const &token);

		event_token SavePointReached(WinUIEditor::SavePointReachedHandler const &handler);
		void SavePointReached(event_token const &token);

		event_token SavePointLeft(WinUIEditor::SavePointLeftHandler const &handler);
		void SavePointLeft(event_token const &token);

		event_token ModifyAttemptRO(WinUIEditor::ModifyAttemptROHandler const &handler);
		void ModifyAttemptRO(event_token const &token);

		event_token Key(WinUIEditor::KeyHandler const &handler);
		void Key(event_token const &token);

		event_token DoubleClick(WinUIEditor::DoubleClickHandler const &handler);
		void DoubleClick(event_token const &token);

		event_token UpdateUI(WinUIEditor::UpdateUIHandler const &handler);
		void UpdateUI(event_token const &token);

		event_token Modified(WinUIEditor::ModifiedHandler const &handler);
		void Modified(event_token const &token);

		event_token MacroRecord(WinUIEditor::MacroRecordHandler const &handler);
		void MacroRecord(event_token const &token);

		event_token MarginClick(WinUIEditor::MarginClickHandler const &handler);
		void MarginClick(event_token const &token);

		event_token NeedShown(WinUIEditor::NeedShownHandler const &handler);
		void NeedShown(event_token const &token);

		event_token Painted(WinUIEditor::PaintedHandler const &handler);
		void Painted(event_token const &token);

		event_token UserListSelection(WinUIEditor::UserListSelectionHandler const &handler);
		void UserListSelection(event_token const &token);

		event_token URIDropped(WinUIEditor::URIDroppedHandler const &handler);
		void URIDropped(event_token const &token);

		event_token DwellStart(WinUIEditor::DwellStartHandler const &handler);
		void DwellStart(event_token const &token);

		event_token DwellEnd(WinUIEditor::DwellEndHandler const &handler);
		void DwellEnd(event_token const &token);

		event_token ZoomChanged(WinUIEditor::ZoomChangedHandler const &handler);
		void ZoomChanged(event_token const &token);

		event_token HotSpotClick(WinUIEditor::HotSpotClickHandler const &handler);
		void HotSpotClick(event_token const &token);

		event_token HotSpotDoubleClick(WinUIEditor::HotSpotDoubleClickHandler const &handler);
		void HotSpotDoubleClick(event_token const &token);

		event_token CallTipClick(WinUIEditor::CallTipClickHandler const &handler);
		void CallTipClick(event_token const &token);

		event_token AutoCSelection(WinUIEditor::AutoCSelectionHandler const &handler);
		void AutoCSelection(event_token const &token);

		event_token IndicatorClick(WinUIEditor::IndicatorClickHandler const &handler);
		void IndicatorClick(event_token const &token);

		event_token IndicatorRelease(WinUIEditor::IndicatorReleaseHandler const &handler);
		void IndicatorRelease(event_token const &token);

		event_token AutoCCancelled(WinUIEditor::AutoCCancelledHandler const &handler);
		void AutoCCancelled(event_token const &token);

		event_token AutoCCharDeleted(WinUIEditor::AutoCCharDeletedHandler const &handler);
		void AutoCCharDeleted(event_token const &token);

		event_token HotSpotReleaseClick(WinUIEditor::HotSpotReleaseClickHandler const &handler);
		void HotSpotReleaseClick(event_token const &token);

		event_token FocusIn(WinUIEditor::FocusInHandler const &handler);
		void FocusIn(event_token const &token);

		event_token FocusOut(WinUIEditor::FocusOutHandler const &handler);
		void FocusOut(event_token const &token);

		event_token AutoCCompleted(WinUIEditor::AutoCCompletedHandler const &handler);
		void AutoCCompleted(event_token const &token);

		event_token MarginRightClick(WinUIEditor::MarginRightClickHandler const &handler);
		void MarginRightClick(event_token const &token);

		event_token AutoCSelectionChange(WinUIEditor::AutoCSelectionChangeHandler const &handler);
		void AutoCSelectionChange(event_token const &token);

		/**
		 * Returns the number of bytes in the document.
		 */
		int64_t Length();

		/**
		 * Returns the position of the caret.
		 */
		int64_t CurrentPos();

		/**
		 * Sets the position of the caret.
		 */
		void CurrentPos(int64_t value);

		/**
		 * Returns the position of the opposite end of the selection to the caret.
		 */
		int64_t Anchor();

		/**
		 * Set the selection anchor to a position. The anchor is the opposite
		 * end of the selection from the caret.
		 */
		void Anchor(int64_t value);

		/**
		 * Is undo history being collected?
		 */
		bool UndoCollection();

		/**
		 * Choose between collecting actions into the undo
		 * history and discarding them.
		 */
		void UndoCollection(bool value);

		/**
		 * Are white space characters currently visible?
		 * Returns one of SCWS_* constants.
		 */
		WinUIEditor::WhiteSpace ViewWS();

		/**
		 * Make white space characters invisible, always visible or visible outside indentation.
		 */
		void ViewWS(WinUIEditor::WhiteSpace const &value);

		/**
		 * Retrieve the current tab draw mode.
		 * Returns one of SCTD_* constants.
		 */
		WinUIEditor::TabDrawMode TabDrawMode();

		/**
		 * Set how tabs are drawn when visible.
		 */
		void TabDrawMode(WinUIEditor::TabDrawMode const &value);

		/**
		 * Retrieve the position of the last correctly styled character.
		 */
		int64_t EndStyled();

		/**
		 * Retrieve the current end of line mode - one of CRLF, CR, or LF.
		 */
		WinUIEditor::EndOfLine EOLMode();

		/**
		 * Set the current end of line mode.
		 */
		void EOLMode(WinUIEditor::EndOfLine const &value);

		/**
		 * Is drawing done first into a buffer or direct to the screen?
		 */
		bool BufferedDraw();

		/**
		 * If drawing is buffered then each line of text is drawn into a bitmap buffer
		 * before drawing it to the screen to avoid flicker.
		 */
		void BufferedDraw(bool value);

		/**
		 * Retrieve the visible size of a tab.
		 */
		int32_t TabWidth();

		/**
		 * Change the visible size of a tab to be a multiple of the width of a space character.
		 */
		void TabWidth(int32_t value);

		/**
		 * Get the minimum visual width of a tab.
		 */
		int32_t TabMinimumWidth();

		/**
		 * Set the minimum visual width of a tab.
		 */
		void TabMinimumWidth(int32_t value);

		/**
		 * Is the IME displayed in a window or inline?
		 */
		WinUIEditor::IMEInteraction IMEInteraction();

		/**
		 * Choose to display the IME in a window or inline.
		 */
		void IMEInteraction(WinUIEditor::IMEInteraction const &value);

		/**
		 * How many margins are there?.
		 */
		int32_t Margins();

		/**
		 * Allocate a non-standard number of margins.
		 */
		void Margins(int32_t value);

		/**
		 * Get the alpha of the selection.
		 */
		WinUIEditor::Alpha SelAlpha();

		/**
		 * Set the alpha of the selection.
		 */
		void SelAlpha(WinUIEditor::Alpha const &value);

		/**
		 * Is the selection end of line filled?
		 */
		bool SelEOLFilled();

		/**
		 * Set the selection to have its end of line filled or not.
		 */
		void SelEOLFilled(bool value);

		/**
		 * Get the layer for drawing selections
		 */
		WinUIEditor::Layer SelectionLayer();

		/**
		 * Set the layer for drawing selections: either opaquely on base layer or translucently over text
		 */
		void SelectionLayer(WinUIEditor::Layer const &value);

		/**
		 * Get the layer of the background of the line containing the caret.
		 */
		WinUIEditor::Layer CaretLineLayer();

		/**
		 * Set the layer of the background of the line containing the caret.
		 */
		void CaretLineLayer(WinUIEditor::Layer const &value);

		/**
		 * Get only highlighting subline instead of whole line.
		 */
		bool CaretLineHighlightSubLine();

		/**
		 * Set only highlighting subline instead of whole line.
		 */
		void CaretLineHighlightSubLine(bool value);

		/**
		 * Get the time in milliseconds that the caret is on and off.
		 */
		int32_t CaretPeriod();

		/**
		 * Get the time in milliseconds that the caret is on and off. 0 = steady on.
		 */
		void CaretPeriod(int32_t value);

		/**
		 * Get the number of characters to have directly indexed categories
		 */
		int32_t CharacterCategoryOptimization();

		/**
		 * Set the number of characters to have directly indexed categories
		 */
		void CharacterCategoryOptimization(int32_t value);

		/**
		 * Is an undo sequence active?
		 */
		int32_t UndoSequence();

		/**
		 * How many undo actions are in the history?
		 */
		int32_t UndoActions();

		/**
		 * Which action is the save point?
		 */
		int32_t UndoSavePoint();

		/**
		 * Set action as the save point
		 */
		void UndoSavePoint(int32_t value);

		/**
		 * Which action is the detach point?
		 */
		int32_t UndoDetach();

		/**
		 * Set action as the detach point
		 */
		void UndoDetach(int32_t value);

		/**
		 * Which action is the tentative point?
		 */
		int32_t UndoTentative();

		/**
		 * Set action as the tentative point
		 */
		void UndoTentative(int32_t value);

		/**
		 * Which action is the current point?
		 */
		int32_t UndoCurrent();

		/**
		 * Set action as the current point
		 */
		void UndoCurrent(int32_t value);

		/**
		 * Get the size of the dots used to mark space characters.
		 */
		int32_t WhitespaceSize();

		/**
		 * Set the size of the dots used to mark space characters.
		 */
		void WhitespaceSize(int32_t value);

		/**
		 * Retrieve the last line number that has line state.
		 */
		int32_t MaxLineState();

		/**
		 * Is the background of the line containing the caret in a different colour?
		 */
		bool CaretLineVisible();

		/**
		 * Display the background of the line containing the caret in a different colour.
		 */
		void CaretLineVisible(bool value);

		/**
		 * Get the colour of the background of the line containing the caret.
		 */
		int32_t CaretLineBack();

		/**
		 * Set the colour of the background of the line containing the caret.
		 */
		void CaretLineBack(int32_t value);

		/**
		 * Retrieve the caret line frame width.
		 * Width = 0 means this option is disabled.
		 */
		int32_t CaretLineFrame();

		/**
		 * Display the caret line framed.
		 * Set width != 0 to enable this option and width = 0 to disable it.
		 */
		void CaretLineFrame(int32_t value);

		/**
		 * Retrieve the auto-completion list separator character.
		 */
		int32_t AutoCSeparator();

		/**
		 * Change the separator character in the string setting up an auto-completion list.
		 * Default is space but can be changed if items contain space.
		 */
		void AutoCSeparator(int32_t value);

		/**
		 * Retrieve whether auto-completion cancelled by backspacing before start.
		 */
		bool AutoCCancelAtStart();

		/**
		 * Should the auto-completion list be cancelled if the user backspaces to a
		 * position before where the box was created.
		 */
		void AutoCCancelAtStart(bool value);

		/**
		 * Retrieve whether a single item auto-completion list automatically choose the item.
		 */
		bool AutoCChooseSingle();

		/**
		 * Should a single item auto-completion list automatically choose the item.
		 */
		void AutoCChooseSingle(bool value);

		/**
		 * Retrieve state of ignore case flag.
		 */
		bool AutoCIgnoreCase();

		/**
		 * Set whether case is significant when performing auto-completion searches.
		 */
		void AutoCIgnoreCase(bool value);

		/**
		 * Retrieve whether or not autocompletion is hidden automatically when nothing matches.
		 */
		bool AutoCAutoHide();

		/**
		 * Set whether or not autocompletion is hidden automatically when nothing matches.
		 */
		void AutoCAutoHide(bool value);

		/**
		 * Retrieve autocompletion options.
		 */
		WinUIEditor::AutoCompleteOption AutoCOptions();

		/**
		 * Set autocompletion options.
		 */
		void AutoCOptions(WinUIEditor::AutoCompleteOption const &value);

		/**
		 * Retrieve whether or not autocompletion deletes any word characters
		 * after the inserted text upon completion.
		 */
		bool AutoCDropRestOfWord();

		/**
		 * Set whether or not autocompletion deletes any word characters
		 * after the inserted text upon completion.
		 */
		void AutoCDropRestOfWord(bool value);

		/**
		 * Retrieve the auto-completion list type-separator character.
		 */
		int32_t AutoCTypeSeparator();

		/**
		 * Change the type-separator character in the string setting up an auto-completion list.
		 * Default is '?' but can be changed if items contain '?'.
		 */
		void AutoCTypeSeparator(int32_t value);

		/**
		 * Get the maximum width, in characters, of auto-completion and user lists.
		 */
		int32_t AutoCMaxWidth();

		/**
		 * Set the maximum width, in characters, of auto-completion and user lists.
		 * Set to 0 to autosize to fit longest item, which is the default.
		 */
		void AutoCMaxWidth(int32_t value);

		/**
		 * Set the maximum height, in rows, of auto-completion and user lists.
		 */
		int32_t AutoCMaxHeight();

		/**
		 * Set the maximum height, in rows, of auto-completion and user lists.
		 * The default is 5 rows.
		 */
		void AutoCMaxHeight(int32_t value);

		/**
		 * Get the style number used for auto-completion and user lists fonts.
		 */
		int32_t AutoCStyle();

		/**
		 * Set the style number used for auto-completion and user lists fonts.
		 */
		void AutoCStyle(int32_t value);

		/**
		 * Get the scale factor in percent for auto-completion list images.
		 */
		int32_t AutoCImageScale();

		/**
		 * Set the scale factor in percent for auto-completion list images.
		 */
		void AutoCImageScale(int32_t value);

		/**
		 * Retrieve indentation size.
		 */
		int32_t Indent();

		/**
		 * Set the number of spaces used for one level of indentation.
		 */
		void Indent(int32_t value);

		/**
		 * Retrieve whether tabs will be used in indentation.
		 */
		bool UseTabs();

		/**
		 * Indentation will only use space characters if useTabs is false, otherwise
		 * it will use a combination of tabs and spaces.
		 */
		void UseTabs(bool value);

		/**
		 * Is the horizontal scroll bar visible?
		 */
		bool HScrollBar();

		/**
		 * Show or hide the horizontal scroll bar.
		 */
		void HScrollBar(bool value);

		/**
		 * Are the indentation guides visible?
		 */
		WinUIEditor::IndentView IndentationGuides();

		/**
		 * Show or hide indentation guides.
		 */
		void IndentationGuides(WinUIEditor::IndentView const &value);

		/**
		 * Get the highlighted indentation guide column.
		 */
		int64_t HighlightGuide();

		/**
		 * Set the highlighted indentation guide column.
		 * 0 = no highlighted guide.
		 */
		void HighlightGuide(int64_t value);

		/**
		 * Get the code page used to interpret the bytes of the document as characters.
		 */
		int32_t CodePage();

		/**
		 * Set the code page used to interpret the bytes of the document as characters.
		 * The SC_CP_UTF8 value can be used to enter Unicode mode.
		 */
		void CodePage(int32_t value);

		/**
		 * Get the foreground colour of the caret.
		 */
		int32_t CaretFore();

		/**
		 * Set the foreground colour of the caret.
		 */
		void CaretFore(int32_t value);

		/**
		 * In read-only mode?
		 */
		bool ReadOnly();

		/**
		 * Set to read only or read write.
		 */
		void ReadOnly(bool value);

		/**
		 * Returns the position at the start of the selection.
		 */
		int64_t SelectionStart();

		/**
		 * Sets the position that starts the selection - this becomes the anchor.
		 */
		void SelectionStart(int64_t value);

		/**
		 * Returns the position at the end of the selection.
		 */
		int64_t SelectionEnd();

		/**
		 * Sets the position that ends the selection - this becomes the caret.
		 */
		void SelectionEnd(int64_t value);

		/**
		 * Returns the print magnification.
		 */
		int32_t PrintMagnification();

		/**
		 * Sets the print magnification added to the point size of each style for printing.
		 */
		void PrintMagnification(int32_t value);

		/**
		 * Returns the print colour mode.
		 */
		WinUIEditor::PrintOption PrintColourMode();

		/**
		 * Modify colours when printing for clearer printed text.
		 */
		void PrintColourMode(WinUIEditor::PrintOption const &value);

		/**
		 * Report change history status.
		 */
		WinUIEditor::ChangeHistoryOption ChangeHistory();

		/**
		 * Enable or disable change history.
		 */
		void ChangeHistory(WinUIEditor::ChangeHistoryOption const &value);

		/**
		 * Report undo selection history status.
		 */
		WinUIEditor::UndoSelectionHistoryOption UndoSelectionHistory();

		/**
		 * Enable or disable undo selection history.
		 */
		void UndoSelectionHistory(WinUIEditor::UndoSelectionHistoryOption const &value);

		/**
		 * Retrieve the display line at the top of the display.
		 */
		int64_t FirstVisibleLine();

		/**
		 * Scroll so that a display line is at the top of the display.
		 */
		void FirstVisibleLine(int64_t value);

		/**
		 * Returns the number of lines in the document. There is always at least one.
		 */
		int64_t LineCount();

		/**
		 * Returns the size in pixels of the left margin.
		 */
		int32_t MarginLeft();

		/**
		 * Sets the size in pixels of the left margin.
		 */
		void MarginLeft(int32_t value);

		/**
		 * Returns the size in pixels of the right margin.
		 */
		int32_t MarginRight();

		/**
		 * Sets the size in pixels of the right margin.
		 */
		void MarginRight(int32_t value);

		/**
		 * Is the document different from when it was last saved?
		 */
		bool Modify();

		bool SelectionHidden();

		/**
		 * Retrieve the number of characters in the document.
		 */
		int64_t TextLength();

		/**
		 * Retrieve a pointer to a function that processes messages for this Scintilla.
		 */
		uint64_t DirectFunction();

		/**
		 * Retrieve a pointer to a function that processes messages for this Scintilla and returns status.
		 */
		uint64_t DirectStatusFunction();

		/**
		 * Retrieve a pointer value to use as the first argument when calling
		 * the function returned by GetDirectFunction.
		 */
		uint64_t DirectPointer();

		/**
		 * Returns true if overtype mode is active otherwise false is returned.
		 */
		bool Overtype();

		/**
		 * Set to overtype (true) or insert mode.
		 */
		void Overtype(bool value);

		/**
		 * Returns the width of the insert mode caret.
		 */
		int32_t CaretWidth();

		/**
		 * Set the width of the insert mode caret.
		 */
		void CaretWidth(int32_t value);

		/**
		 * Get the position that starts the target.
		 */
		int64_t TargetStart();

		/**
		 * Sets the position that starts the target which is used for updating the
		 * document without affecting the scroll position.
		 */
		void TargetStart(int64_t value);

		/**
		 * Get the virtual space of the target start
		 */
		int64_t TargetStartVirtualSpace();

		/**
		 * Sets the virtual space of the target start
		 */
		void TargetStartVirtualSpace(int64_t value);

		/**
		 * Get the position that ends the target.
		 */
		int64_t TargetEnd();

		/**
		 * Sets the position that ends the target which is used for updating the
		 * document without affecting the scroll position.
		 */
		void TargetEnd(int64_t value);

		/**
		 * Get the virtual space of the target end
		 */
		int64_t TargetEndVirtualSpace();

		/**
		 * Sets the virtual space of the target end
		 */
		void TargetEndVirtualSpace(int64_t value);

		/**
		 * Get the search flags used by SearchInTarget.
		 */
		WinUIEditor::FindOption SearchFlags();

		/**
		 * Set the search flags used by SearchInTarget.
		 */
		void SearchFlags(WinUIEditor::FindOption const &value);

		/**
		 * Are all lines visible?
		 */
		bool AllLinesVisible();

		/**
		 * Get the style of fold display text.
		 */
		WinUIEditor::FoldDisplayTextStyle FoldDisplayTextStyle();

		/**
		 * Set the style of fold display text.
		 */
		void FoldDisplayTextStyle(WinUIEditor::FoldDisplayTextStyle const &value);

		/**
		 * Get automatic folding behaviours.
		 */
		WinUIEditor::AutomaticFold AutomaticFold();

		/**
		 * Set automatic folding behaviours.
		 */
		void AutomaticFold(WinUIEditor::AutomaticFold const &value);

		/**
		 * Does a tab pressed when caret is within indentation indent?
		 */
		bool TabIndents();

		/**
		 * Sets whether a tab pressed when caret is within indentation indents.
		 */
		void TabIndents(bool value);

		/**
		 * Does a backspace pressed when caret is within indentation unindent?
		 */
		bool BackSpaceUnIndents();

		/**
		 * Sets whether a backspace pressed when caret is within indentation unindents.
		 */
		void BackSpaceUnIndents(bool value);

		/**
		 * Retrieve the time the mouse must sit still to generate a mouse dwell event.
		 */
		int32_t MouseDwellTime();

		/**
		 * Sets the time the mouse must sit still to generate a mouse dwell event.
		 */
		void MouseDwellTime(int32_t value);

		/**
		 * Retrieve the limits to idle styling.
		 */
		WinUIEditor::IdleStyling IdleStyling();

		/**
		 * Sets limits to idle styling.
		 */
		void IdleStyling(WinUIEditor::IdleStyling const &value);

		/**
		 * Retrieve whether text is word wrapped.
		 */
		WinUIEditor::Wrap WrapMode();

		/**
		 * Sets whether text is word wrapped.
		 */
		void WrapMode(WinUIEditor::Wrap const &value);

		/**
		 * Retrive the display mode of visual flags for wrapped lines.
		 */
		WinUIEditor::WrapVisualFlag WrapVisualFlags();

		/**
		 * Set the display mode of visual flags for wrapped lines.
		 */
		void WrapVisualFlags(WinUIEditor::WrapVisualFlag const &value);

		/**
		 * Retrive the location of visual flags for wrapped lines.
		 */
		WinUIEditor::WrapVisualLocation WrapVisualFlagsLocation();

		/**
		 * Set the location of visual flags for wrapped lines.
		 */
		void WrapVisualFlagsLocation(WinUIEditor::WrapVisualLocation const &value);

		/**
		 * Retrive the start indent for wrapped lines.
		 */
		int32_t WrapStartIndent();

		/**
		 * Set the start indent for wrapped lines.
		 */
		void WrapStartIndent(int32_t value);

		/**
		 * Retrieve how wrapped sublines are placed. Default is fixed.
		 */
		WinUIEditor::WrapIndentMode WrapIndentMode();

		/**
		 * Sets how wrapped sublines are placed. Default is fixed.
		 */
		void WrapIndentMode(WinUIEditor::WrapIndentMode const &value);

		/**
		 * Retrieve the degree of caching of layout information.
		 */
		WinUIEditor::LineCache LayoutCache();

		/**
		 * Sets the degree of caching of layout information.
		 */
		void LayoutCache(WinUIEditor::LineCache const &value);

		/**
		 * Retrieve the document width assumed for scrolling.
		 */
		int32_t ScrollWidth();

		/**
		 * Sets the document width assumed for scrolling.
		 */
		void ScrollWidth(int32_t value);

		/**
		 * Retrieve whether the scroll width tracks wide lines.
		 */
		bool ScrollWidthTracking();

		/**
		 * Sets whether the maximum width line displayed is used to set scroll width.
		 */
		void ScrollWidthTracking(bool value);

		/**
		 * Retrieve whether the maximum scroll position has the last
		 * line at the bottom of the view.
		 */
		bool EndAtLastLine();

		/**
		 * Sets the scroll range so that maximum scroll position has
		 * the last line at the bottom of the view (default).
		 * Setting this to false allows scrolling one page below the last line.
		 */
		void EndAtLastLine(bool value);

		/**
		 * Is the vertical scroll bar visible?
		 */
		bool VScrollBar();

		/**
		 * Show or hide the vertical scroll bar.
		 */
		void VScrollBar(bool value);

		/**
		 * How many phases is drawing done in?
		 */
		WinUIEditor::PhasesDraw PhasesDraw();

		/**
		 * In one phase draw, text is drawn in a series of rectangular blocks with no overlap.
		 * In two phase draw, text is drawn in a series of lines allowing runs to overlap horizontally.
		 * In multiple phase draw, each element is drawn over the whole drawing area, allowing text
		 * to overlap from one line to the next.
		 */
		void PhasesDraw(WinUIEditor::PhasesDraw const &value);

		/**
		 * Retrieve the quality level for text.
		 */
		WinUIEditor::FontQuality FontQuality();

		/**
		 * Choose the quality level for text from the FontQuality enumeration.
		 */
		void FontQuality(WinUIEditor::FontQuality const &value);

		/**
		 * Retrieve the effect of pasting when there are multiple selections.
		 */
		WinUIEditor::MultiPaste MultiPaste();

		/**
		 * Change the effect of pasting when there are multiple selections.
		 */
		void MultiPaste(WinUIEditor::MultiPaste const &value);

		/**
		 * Report accessibility status.
		 */
		WinUIEditor::Accessibility Accessibility();

		/**
		 * Enable or disable accessibility.
		 */
		void Accessibility(WinUIEditor::Accessibility const &value);

		/**
		 * Are the end of line characters visible?
		 */
		bool ViewEOL();

		/**
		 * Make the end of line characters visible or invisible.
		 */
		void ViewEOL(bool value);

		/**
		 * Retrieve a pointer to the document object.
		 */
		uint64_t DocPointer();

		/**
		 * Change the document object used.
		 */
		void DocPointer(uint64_t value);

		/**
		 * Retrieve the column number which text should be kept within.
		 */
		int64_t EdgeColumn();

		/**
		 * Set the column number of the edge.
		 * If text goes past the edge then it is highlighted.
		 */
		void EdgeColumn(int64_t value);

		/**
		 * Retrieve the edge highlight mode.
		 */
		WinUIEditor::EdgeVisualStyle EdgeMode();

		/**
		 * The edge may be displayed by a line (EDGE_LINE/EDGE_MULTILINE) or by highlighting text that
		 * goes beyond it (EDGE_BACKGROUND) or not displayed at all (EDGE_NONE).
		 */
		void EdgeMode(WinUIEditor::EdgeVisualStyle const &value);

		/**
		 * Retrieve the colour used in edge indication.
		 */
		int32_t EdgeColour();

		/**
		 * Change the colour used in edge indication.
		 */
		void EdgeColour(int32_t value);

		/**
		 * Retrieves the number of lines completely visible.
		 */
		int64_t LinesOnScreen();

		/**
		 * Is the selection rectangular? The alternative is the more common stream selection.
		 */
		bool SelectionIsRectangle();

		/**
		 * Retrieve the zoom level.
		 */
		int32_t Zoom();

		/**
		 * Set the zoom level. This number of points is added to the size of all fonts.
		 * It may be positive to magnify or negative to reduce.
		 */
		void Zoom(int32_t value);

		/**
		 * Get which document options are set.
		 */
		WinUIEditor::DocumentOption DocumentOptions();

		/**
		 * Get which document modification events are sent to the container.
		 */
		WinUIEditor::ModificationFlags ModEventMask();

		/**
		 * Set which document modification events are sent to the container.
		 */
		void ModEventMask(WinUIEditor::ModificationFlags const &value);

		/**
		 * Get whether command events are sent to the container.
		 */
		bool CommandEvents();

		/**
		 * Set whether command events are sent to the container.
		 */
		void CommandEvents(bool value);

		/**
		 * Get internal focus flag.
		 */
		bool Focus();

		/**
		 * Change internal focus flag.
		 */
		void Focus(bool value);

		/**
		 * Get error status.
		 */
		WinUIEditor::Status Status();

		/**
		 * Change error status - 0 = OK.
		 */
		void Status(WinUIEditor::Status const &value);

		/**
		 * Get whether mouse gets captured.
		 */
		bool MouseDownCaptures();

		/**
		 * Set whether the mouse is captured when its button is pressed.
		 */
		void MouseDownCaptures(bool value);

		/**
		 * Get whether mouse wheel can be active outside the window.
		 */
		bool MouseWheelCaptures();

		/**
		 * Set whether the mouse wheel can be active outside the window.
		 */
		void MouseWheelCaptures(bool value);

		/**
		 * Get cursor type.
		 */
		WinUIEditor::CursorShape Cursor();

		/**
		 * Sets the cursor to one of the SC_CURSOR* values.
		 */
		void Cursor(WinUIEditor::CursorShape const &value);

		/**
		 * Get the way control characters are displayed.
		 */
		int32_t ControlCharSymbol();

		/**
		 * Change the way control characters are displayed:
		 * If symbol is < 32, keep the drawn way, else, use the given character.
		 */
		void ControlCharSymbol(int32_t value);

		/**
		 * Get the xOffset (ie, horizontal scroll position).
		 */
		int32_t XOffset();

		/**
		 * Set the xOffset (ie, horizontal scroll position).
		 */
		void XOffset(int32_t value);

		/**
		 * Is printing line wrapped?
		 */
		WinUIEditor::Wrap PrintWrapMode();

		/**
		 * Set printing to line wrapped (SC_WRAP_WORD) or not line wrapped (SC_WRAP_NONE).
		 */
		void PrintWrapMode(WinUIEditor::Wrap const &value);

		/**
		 * Get whether underlining for active hotspots.
		 */
		bool HotspotActiveUnderline();

		/**
		 * Enable / Disable underlining active hotspots.
		 */
		void HotspotActiveUnderline(bool value);

		/**
		 * Get the HotspotSingleLine property
		 */
		bool HotspotSingleLine();

		/**
		 * Limit hotspots to single line so hotspots on two lines don't merge.
		 */
		void HotspotSingleLine(bool value);

		/**
		 * Get the mode of the current selection.
		 */
		WinUIEditor::SelectionMode SelectionMode();

		/**
		 * Set the selection mode to stream (SC_SEL_STREAM) or rectangular (SC_SEL_RECTANGLE/SC_SEL_THIN) or
		 * by lines (SC_SEL_LINES).
		 */
		void SelectionMode(WinUIEditor::SelectionMode const &value);

		/**
		 * Get whether or not regular caret moves will extend or reduce the selection.
		 */
		bool MoveExtendsSelection();

		/**
		 * Set whether or not regular caret moves will extend or reduce the selection.
		 */
		void MoveExtendsSelection(bool value);

		/**
		 * Get currently selected item position in the auto-completion list
		 */
		int32_t AutoCCurrent();

		/**
		 * Get auto-completion case insensitive behaviour.
		 */
		WinUIEditor::CaseInsensitiveBehaviour AutoCCaseInsensitiveBehaviour();

		/**
		 * Set auto-completion case insensitive behaviour to either prefer case-sensitive matches or have no preference.
		 */
		void AutoCCaseInsensitiveBehaviour(WinUIEditor::CaseInsensitiveBehaviour const &value);

		/**
		 * Retrieve the effect of autocompleting when there are multiple selections.
		 */
		WinUIEditor::MultiAutoComplete AutoCMulti();

		/**
		 * Change the effect of autocompleting when there are multiple selections.
		 */
		void AutoCMulti(WinUIEditor::MultiAutoComplete const &value);

		/**
		 * Get the way autocompletion lists are ordered.
		 */
		WinUIEditor::Ordering AutoCOrder();

		/**
		 * Set the way autocompletion lists are ordered.
		 */
		void AutoCOrder(WinUIEditor::Ordering const &value);

		/**
		 * Can the caret preferred x position only be changed by explicit movement commands?
		 */
		WinUIEditor::CaretSticky CaretSticky();

		/**
		 * Stop the caret preferred x position changing when the user types.
		 */
		void CaretSticky(WinUIEditor::CaretSticky const &value);

		/**
		 * Get convert-on-paste setting
		 */
		bool PasteConvertEndings();

		/**
		 * Enable/Disable convert-on-paste for line endings
		 */
		void PasteConvertEndings(bool value);

		/**
		 * Get the background alpha of the caret line.
		 */
		WinUIEditor::Alpha CaretLineBackAlpha();

		/**
		 * Set background alpha of the caret line.
		 */
		void CaretLineBackAlpha(WinUIEditor::Alpha const &value);

		/**
		 * Returns the current style of the caret.
		 */
		WinUIEditor::CaretStyle CaretStyle();

		/**
		 * Set the style of the caret to be drawn.
		 */
		void CaretStyle(WinUIEditor::CaretStyle const &value);

		/**
		 * Get the current indicator
		 */
		int32_t IndicatorCurrent();

		/**
		 * Set the indicator used for IndicatorFillRange and IndicatorClearRange
		 */
		void IndicatorCurrent(int32_t value);

		/**
		 * Get the current indicator value
		 */
		int32_t IndicatorValue();

		/**
		 * Set the value used for IndicatorFillRange
		 */
		void IndicatorValue(int32_t value);

		/**
		 * How many entries are allocated to the position cache?
		 */
		int32_t PositionCache();

		/**
		 * Set number of entries in position cache
		 */
		void PositionCache(int32_t value);

		/**
		 * Get maximum number of threads used for layout
		 */
		int32_t LayoutThreads();

		/**
		 * Set maximum number of threads used for layout
		 */
		void LayoutThreads(int32_t value);

		/**
		 * Compact the document buffer and return a read-only pointer to the
		 * characters in the document.
		 */
		uint64_t CharacterPointer();

		/**
		 * Return a position which, to avoid performance costs, should not be within
		 * the range of a call to GetRangePointer.
		 */
		int64_t GapPosition();

		/**
		 * Get extra ascent for each line
		 */
		int32_t ExtraAscent();

		/**
		 * Set extra ascent for each line
		 */
		void ExtraAscent(int32_t value);

		/**
		 * Get extra descent for each line
		 */
		int32_t ExtraDescent();

		/**
		 * Set extra descent for each line
		 */
		void ExtraDescent(int32_t value);

		/**
		 * Get the start of the range of style numbers used for margin text
		 */
		int32_t MarginStyleOffset();

		/**
		 * Get the start of the range of style numbers used for margin text
		 */
		void MarginStyleOffset(int32_t value);

		/**
		 * Get the margin options.
		 */
		WinUIEditor::MarginOption MarginOptions();

		/**
		 * Set the margin options.
		 */
		void MarginOptions(WinUIEditor::MarginOption const &value);

		/**
		 * Get the visibility for the annotations for a view
		 */
		WinUIEditor::AnnotationVisible AnnotationVisible();

		/**
		 * Set the visibility for the annotations for a view
		 */
		void AnnotationVisible(WinUIEditor::AnnotationVisible const &value);

		/**
		 * Get the start of the range of style numbers used for annotations
		 */
		int32_t AnnotationStyleOffset();

		/**
		 * Get the start of the range of style numbers used for annotations
		 */
		void AnnotationStyleOffset(int32_t value);

		/**
		 * Whether switching to rectangular mode while selecting with the mouse is allowed.
		 */
		bool MouseSelectionRectangularSwitch();

		/**
		 * Set whether switching to rectangular mode while selecting with the mouse is allowed.
		 */
		void MouseSelectionRectangularSwitch(bool value);

		/**
		 * Whether multiple selections can be made
		 */
		bool MultipleSelection();

		/**
		 * Set whether multiple selections can be made
		 */
		void MultipleSelection(bool value);

		/**
		 * Whether typing can be performed into multiple selections
		 */
		bool AdditionalSelectionTyping();

		/**
		 * Set whether typing can be performed into multiple selections
		 */
		void AdditionalSelectionTyping(bool value);

		/**
		 * Whether additional carets will blink
		 */
		bool AdditionalCaretsBlink();

		/**
		 * Set whether additional carets will blink
		 */
		void AdditionalCaretsBlink(bool value);

		/**
		 * Whether additional carets are visible
		 */
		bool AdditionalCaretsVisible();

		/**
		 * Set whether additional carets are visible
		 */
		void AdditionalCaretsVisible(bool value);

		/**
		 * How many selections are there?
		 */
		int32_t Selections();

		/**
		 * Is every selected range empty?
		 */
		bool SelectionEmpty();

		/**
		 * Which selection is the main selection
		 */
		int32_t MainSelection();

		/**
		 * Set the main selection
		 */
		void MainSelection(int32_t value);

		/**
		 * Return the caret position of the rectangular selection.
		 */
		int64_t RectangularSelectionCaret();

		/**
		 * Set the caret position of the rectangular selection.
		 */
		void RectangularSelectionCaret(int64_t value);

		/**
		 * Return the anchor position of the rectangular selection.
		 */
		int64_t RectangularSelectionAnchor();

		/**
		 * Set the anchor position of the rectangular selection.
		 */
		void RectangularSelectionAnchor(int64_t value);

		/**
		 * Return the virtual space of the caret of the rectangular selection.
		 */
		int64_t RectangularSelectionCaretVirtualSpace();

		/**
		 * Set the virtual space of the caret of the rectangular selection.
		 */
		void RectangularSelectionCaretVirtualSpace(int64_t value);

		/**
		 * Return the virtual space of the anchor of the rectangular selection.
		 */
		int64_t RectangularSelectionAnchorVirtualSpace();

		/**
		 * Set the virtual space of the anchor of the rectangular selection.
		 */
		void RectangularSelectionAnchorVirtualSpace(int64_t value);

		/**
		 * Return options for virtual space behaviour.
		 */
		WinUIEditor::VirtualSpace VirtualSpaceOptions();

		/**
		 * Set options for virtual space behaviour.
		 */
		void VirtualSpaceOptions(WinUIEditor::VirtualSpace const &value);

		/**
		 * Get the modifier key used for rectangular selection.
		 */
		int32_t RectangularSelectionModifier();

		void RectangularSelectionModifier(int32_t value);

		/**
		 * Get the alpha of the selection.
		 */
		WinUIEditor::Alpha AdditionalSelAlpha();

		/**
		 * Set the alpha of the selection.
		 */
		void AdditionalSelAlpha(WinUIEditor::Alpha const &value);

		/**
		 * Get the foreground colour of additional carets.
		 */
		int32_t AdditionalCaretFore();

		/**
		 * Set the foreground colour of additional carets.
		 */
		void AdditionalCaretFore(int32_t value);

		/**
		 * Get the identifier.
		 */
		int32_t Identifier();

		/**
		 * Set the identifier reported as idFrom in notification messages.
		 */
		void Identifier(int32_t value);

		/**
		 * Get the tech.
		 */
		WinUIEditor::Technology Technology();

		/**
		 * Set the technology used.
		 */
		void Technology(WinUIEditor::Technology const &value);

		/**
		 * Is the caret line always visible?
		 */
		bool CaretLineVisibleAlways();

		/**
		 * Sets the caret line to always visible.
		 */
		void CaretLineVisibleAlways(bool value);

		/**
		 * Get the line end types currently allowed.
		 */
		WinUIEditor::LineEndType LineEndTypesAllowed();

		/**
		 * Set the line end types that the application wants to use. May not be used if incompatible with lexer or encoding.
		 */
		void LineEndTypesAllowed(WinUIEditor::LineEndType const &value);

		/**
		 * Get the line end types currently recognised. May be a subset of the allowed types due to lexer limitation.
		 */
		WinUIEditor::LineEndType LineEndTypesActive();

		/**
		 * Get the visibility for the end of line annotations for a view
		 */
		WinUIEditor::EOLAnnotationVisible EOLAnnotationVisible();

		/**
		 * Set the visibility for the end of line annotations for a view
		 */
		void EOLAnnotationVisible(WinUIEditor::EOLAnnotationVisible const &value);

		/**
		 * Get the start of the range of style numbers used for end of line annotations
		 */
		int32_t EOLAnnotationStyleOffset();

		/**
		 * Get the start of the range of style numbers used for end of line annotations
		 */
		void EOLAnnotationStyleOffset(int32_t value);

		/**
		 * Retrieve line character index state.
		 */
		WinUIEditor::LineCharacterIndexType LineCharacterIndex();

		/**
		 * Get whether drag-and-drop is enabled or disabled
		 */
		bool DragDropEnabled();

		/**
		 * Enable or disable drag-and-drop
		 */
		void DragDropEnabled(bool value);

		/**
		 * Retrieve the lexing language of the document.
		 */
		int32_t Lexer();

		/**
		 * Bit set of LineEndType enumertion for which line ends beyond the standard
		 * LF, CR, and CRLF are supported by the lexer.
		 */
		WinUIEditor::LineEndType LineEndTypesSupported();

		/**
		 * Where styles are duplicated by a feature such as active/inactive code
		 * return the distance between the two types.
		 */
		int32_t DistanceToSecondaryStyles();

		/**
		 * Retrieve the number of named styles for the lexer.
		 */
		int32_t NamedStyles();

		/**
		 * Retrieve bidirectional text display state.
		 */
		WinUIEditor::Bidirectional Bidirectional();

		/**
		 * Set bidirectional text display state.
		 */
		void Bidirectional(WinUIEditor::Bidirectional const &value);

		/**
		 * Add text to the document at current position.
		 */
		void AddTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		void AddText(int64_t length, hstring const &text);

		/**
		 * Add array of cells to document.
		 */
		void AddStyledText(int64_t length, array_view<uint8_t const> c);

		/**
		 * Insert string at a position.
		 */
		void InsertTextFromBuffer(int64_t pos, Windows::Storage::Streams::IBuffer const &text);
		void InsertText(int64_t pos, hstring const &text);

		/**
		 * Change the text that is being inserted in response to SC_MOD_INSERTCHECK
		 */
		void ChangeInsertionFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		void ChangeInsertion(int64_t length, hstring const &text);

		/**
		 * Delete all text in the document.
		 */
		void ClearAll();

		/**
		 * Delete a range of text in the document.
		 */
		void DeleteRange(int64_t start, int64_t lengthDelete);

		/**
		 * Set all style bytes to 0, remove all folding information.
		 */
		void ClearDocumentStyle();

		/**
		 * Redoes the next action on the undo history.
		 */
		void Redo();

		/**
		 * Select all the text in the document.
		 */
		void SelectAll();

		/**
		 * Remember the current position in the undo history as the position
		 * at which the document was saved.
		 */
		void SetSavePoint();

		/**
		 * Retrieve a buffer of cells.
		 * Returns the number of bytes in the buffer not including terminating NULs.
		 */
		int64_t GetStyledText(uint64_t tr);

		/**
		 * Retrieve a buffer of cells that can be past 2GB.
		 * Returns the number of bytes in the buffer not including terminating NULs.
		 */
		int64_t GetStyledTextFull(uint64_t tr);

		/**
		 * Are there any redoable actions in the undo history?
		 */
		bool CanRedo();

		/**
		 * Retrieve the line number at which a particular marker is located.
		 */
		int64_t MarkerLineFromHandle(int32_t markerHandle);

		/**
		 * Delete a marker.
		 */
		void MarkerDeleteHandle(int32_t markerHandle);

		/**
		 * Retrieve marker handles of a line
		 */
		int32_t MarkerHandleFromLine(int64_t line, int32_t which);

		/**
		 * Retrieve marker number of a marker handle
		 */
		int32_t MarkerNumberFromLine(int64_t line, int32_t which);

		/**
		 * Find the position from a point within the window.
		 */
		int64_t PositionFromPoint(int32_t x, int32_t y);

		/**
		 * Find the position from a point within the window but return
		 * INVALID_POSITION if not close to text.
		 */
		int64_t PositionFromPointClose(int32_t x, int32_t y);

		/**
		 * Set caret to start of a line and ensure it is visible.
		 */
		void GotoLine(int64_t line);

		/**
		 * Set caret to a position and ensure it is visible.
		 */
		void GotoPos(int64_t caret);

		/**
		 * Retrieve the text of the line containing the caret.
		 * Returns the index of the caret on the line.
		 * Result is NUL-terminated.
		 */
		int64_t GetCurLineWriteBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		hstring GetCurLine(int64_t length);

		/**
		 * Convert all line endings in the document to one mode.
		 */
		void ConvertEOLs(WinUIEditor::EndOfLine const &eolMode);

		/**
		 * Set the current styling position to start.
		 * The unused parameter is no longer used and should be set to 0.
		 */
		void StartStyling(int64_t start, int32_t unused);

		/**
		 * Change style from current styling position for length characters to a style
		 * and move the current styling position to after this newly styled segment.
		 */
		void SetStyling(int64_t length, int32_t style);

		/**
		 * Clear explicit tabstops on a line.
		 */
		void ClearTabStops(int64_t line);

		/**
		 * Add an explicit tab stop for a line.
		 */
		void AddTabStop(int64_t line, int32_t x);

		/**
		 * Find the next explicit tab stop position on a line after a position.
		 */
		int32_t GetNextTabStop(int64_t line, int32_t x);

		/**
		 * Set the symbol used for a particular marker number.
		 */
		void MarkerDefine(int32_t markerNumber, WinUIEditor::MarkerSymbol const &markerSymbol);

		/**
		 * Enable/disable highlight for current folding block (smallest one that contains the caret)
		 */
		void MarkerEnableHighlight(bool enabled);

		/**
		 * Add a marker to a line, returning an ID which can be used to find or delete the marker.
		 */
		int32_t MarkerAdd(int64_t line, int32_t markerNumber);

		/**
		 * Delete a marker from a line.
		 */
		void MarkerDelete(int64_t line, int32_t markerNumber);

		/**
		 * Delete all markers with a particular number from all lines.
		 */
		void MarkerDeleteAll(int32_t markerNumber);

		/**
		 * Get a bit mask of all the markers set on a line.
		 */
		int32_t MarkerGet(int64_t line);

		/**
		 * Find the next line at or after lineStart that includes a marker in mask.
		 * Return -1 when no more lines.
		 */
		int64_t MarkerNext(int64_t lineStart, int32_t markerMask);

		/**
		 * Find the previous line before lineStart that includes a marker in mask.
		 */
		int64_t MarkerPrevious(int64_t lineStart, int32_t markerMask);

		/**
		 * Define a marker from a pixmap.
		 */
		void MarkerDefinePixmapFromBuffer(int32_t markerNumber, Windows::Storage::Streams::IBuffer const &pixmap);
		void MarkerDefinePixmap(int32_t markerNumber, hstring const &pixmap);

		/**
		 * Add a set of markers to a line.
		 */
		void MarkerAddSet(int64_t line, int32_t markerSet);

		/**
		 * Clear all the styles and make equivalent to the global default style.
		 */
		void StyleClearAll();

		/**
		 * Reset the default style to its state at startup
		 */
		void StyleResetDefault();

		/**
		 * Use the default or platform-defined colour for an element.
		 */
		void ResetElementColour(WinUIEditor::Element const &element);

		/**
		 * Set the foreground colour of the main and additional selections and whether to use this setting.
		 */
		void SetSelFore(bool useSetting, int32_t fore);

		/**
		 * Set the background colour of the main and additional selections and whether to use this setting.
		 */
		void SetSelBack(bool useSetting, int32_t back);

		/**
		 * When key+modifier combination keyDefinition is pressed perform sciCommand.
		 */
		void AssignCmdKey(int32_t keyDefinition, int32_t sciCommand);

		/**
		 * When key+modifier combination keyDefinition is pressed do nothing.
		 */
		void ClearCmdKey(int32_t keyDefinition);

		/**
		 * Drop all key mappings.
		 */
		void ClearAllCmdKeys();

		/**
		 * Set the styles for a segment of the document.
		 */
		void SetStylingExFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &styles);
		void SetStylingEx(int64_t length, hstring const &styles);

		/**
		 * Start a sequence of actions that is undone and redone as a unit.
		 * May be nested.
		 */
		void BeginUndoAction();

		/**
		 * End a sequence of actions that is undone and redone as a unit.
		 */
		void EndUndoAction();

		/**
		 * Push one action onto undo history with no text
		 */
		void PushUndoActionType(int32_t type, int64_t pos);

		/**
		 * Set the text and length of the most recently pushed action
		 */
		void ChangeLastUndoActionTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		void ChangeLastUndoActionText(int64_t length, hstring const &text);

		/**
		 * Set the foreground colour of all whitespace and whether to use this setting.
		 */
		void SetWhitespaceFore(bool useSetting, int32_t fore);

		/**
		 * Set the background colour of all whitespace and whether to use this setting.
		 */
		void SetWhitespaceBack(bool useSetting, int32_t back);

		/**
		 * Display a auto-completion list.
		 * The lengthEntered parameter indicates how many characters before
		 * the caret should be used to provide context.
		 */
		void AutoCShowFromBuffer(int64_t lengthEntered, Windows::Storage::Streams::IBuffer const &itemList);
		void AutoCShow(int64_t lengthEntered, hstring const &itemList);

		/**
		 * Remove the auto-completion list from the screen.
		 */
		void AutoCCancel();

		/**
		 * Is there an auto-completion list visible?
		 */
		bool AutoCActive();

		/**
		 * Retrieve the position of the caret when the auto-completion list was displayed.
		 */
		int64_t AutoCPosStart();

		/**
		 * User has selected an item so remove the list and insert the selection.
		 */
		void AutoCComplete();

		/**
		 * Define a set of character that when typed cancel the auto-completion list.
		 */
		void AutoCStopsFromBuffer(Windows::Storage::Streams::IBuffer const &characterSet);
		void AutoCStops(hstring const &characterSet);

		/**
		 * Select the item in the auto-completion list that starts with a string.
		 */
		void AutoCSelectFromBuffer(Windows::Storage::Streams::IBuffer const &select);
		void AutoCSelect(hstring const &select);

		/**
		 * Display a list of strings and send notification when user chooses one.
		 */
		void UserListShowFromBuffer(int32_t listType, Windows::Storage::Streams::IBuffer const &itemList);
		void UserListShow(int32_t listType, hstring const &itemList);

		/**
		 * Register an XPM image for use in autocompletion lists.
		 */
		void RegisterImageFromBuffer(int32_t type, Windows::Storage::Streams::IBuffer const &xpmData);
		void RegisterImage(int32_t type, hstring const &xpmData);

		/**
		 * Clear all the registered XPM images.
		 */
		void ClearRegisteredImages();

		/**
		 * Count characters between two positions.
		 */
		int64_t CountCharacters(int64_t start, int64_t end);

		/**
		 * Count code units between two positions.
		 */
		int64_t CountCodeUnits(int64_t start, int64_t end);

		/**
		 * Set caret to a position, while removing any existing selection.
		 */
		void SetEmptySelection(int64_t caret);

		/**
		 * Find some text in the document.
		 */
		int64_t FindText(WinUIEditor::FindOption const &searchFlags, uint64_t ft);

		/**
		 * Find some text in the document.
		 */
		int64_t FindTextFull(WinUIEditor::FindOption const &searchFlags, uint64_t ft);

		/**
		 * Draw the document into a display context such as a printer.
		 */
		int64_t FormatRange(bool draw, uint64_t fr);

		/**
		 * Draw the document into a display context such as a printer.
		 */
		int64_t FormatRangeFull(bool draw, uint64_t fr);

		/**
		 * Retrieve the contents of a line.
		 * Returns the length of the line.
		 */
		int64_t GetLineWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		hstring GetLine(int64_t line);

		/**
		 * Select a range of text.
		 */
		void SetSel(int64_t anchor, int64_t caret);

		/**
		 * Retrieve the selected text.
		 * Return the length of the text.
		 * Result is NUL-terminated.
		 */
		int64_t GetSelTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text);
		hstring GetSelText();

		/**
		 * Retrieve a range of text.
		 * Return the length of the text.
		 */
		int64_t GetTextRange(uint64_t tr);

		/**
		 * Retrieve a range of text that can be past 2GB.
		 * Return the length of the text.
		 */
		int64_t GetTextRangeFull(uint64_t tr);

		/**
		 * Draw the selection either highlighted or in normal (non-highlighted) style.
		 */
		void HideSelection(bool hide);

		/**
		 * Retrieve the x value of the point in the window where a position is displayed.
		 */
		int32_t PointXFromPosition(int64_t pos);

		/**
		 * Retrieve the y value of the point in the window where a position is displayed.
		 */
		int32_t PointYFromPosition(int64_t pos);

		/**
		 * Retrieve the line containing a position.
		 */
		int64_t LineFromPosition(int64_t pos);

		/**
		 * Retrieve the position at the start of a line.
		 */
		int64_t PositionFromLine(int64_t line);

		/**
		 * Scroll horizontally and vertically.
		 */
		void LineScroll(int64_t columns, int64_t lines);

		/**
		 * Scroll vertically with allowance for wrapping.
		 */
		void ScrollVertical(int64_t docLine, int64_t subLine);

		/**
		 * Ensure the caret is visible.
		 */
		void ScrollCaret();

		/**
		 * Scroll the argument positions and the range between them into view giving
		 * priority to the primary position then the secondary position.
		 * This may be used to make a search match visible.
		 */
		void ScrollRange(int64_t secondary, int64_t primary);

		/**
		 * Replace the selected text with the argument text.
		 */
		void ReplaceSelFromBuffer(Windows::Storage::Streams::IBuffer const &text);
		void ReplaceSel(hstring const &text);

		/**
		 * Null operation.
		 */
		void Null();

		/**
		 * Will a paste succeed?
		 */
		bool CanPaste();

		/**
		 * Are there any undoable actions in the undo history?
		 */
		bool CanUndo();

		/**
		 * Delete the undo history.
		 */
		void EmptyUndoBuffer();

		/**
		 * Undo one action in the undo history.
		 */
		void Undo();

		/**
		 * Cut the selection to the clipboard.
		 */
		void Cut();

		/**
		 * Copy the selection to the clipboard.
		 */
		void Copy();

		/**
		 * Paste the contents of the clipboard into the document replacing the selection.
		 */
		void Paste();

		/**
		 * Clear the selection.
		 */
		void Clear();

		/**
		 * Replace the contents of the document with the argument text.
		 */
		void SetTextFromBuffer(Windows::Storage::Streams::IBuffer const &text);
		void SetText(hstring const &text);

		/**
		 * Retrieve all the text in the document.
		 * Returns number of characters retrieved.
		 * Result is NUL-terminated.
		 */
		int64_t GetTextWriteBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		hstring GetText(int64_t length);

		/**
		 * Sets both the start and end of the target in one call.
		 */
		void SetTargetRange(int64_t start, int64_t end);

		/**
		 * Make the target range start and end be the same as the selection range start and end.
		 */
		void TargetFromSelection();

		/**
		 * Sets the target to the whole document.
		 */
		void TargetWholeDocument();

		/**
		 * Replace the target text with the argument text.
		 * Text is counted so it can contain NULs.
		 * Returns the length of the replacement text.
		 */
		int64_t ReplaceTargetFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		int64_t ReplaceTarget(int64_t length, hstring const &text);

		/**
		 * Replace the target text with the argument text after \d processing.
		 * Text is counted so it can contain NULs.
		 * Looks for \d where d is between 1 and 9 and replaces these with the strings
		 * matched in the last search operation which were surrounded by \( and \).
		 * Returns the length of the replacement text including any change
		 * caused by processing the \d patterns.
		 */
		int64_t ReplaceTargetREFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		int64_t ReplaceTargetRE(int64_t length, hstring const &text);

		/**
		 * Replace the target text with the argument text but ignore prefix and suffix that
		 * are the same as current.
		 */
		int64_t ReplaceTargetMinimalFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		int64_t ReplaceTargetMinimal(int64_t length, hstring const &text);

		/**
		 * Search for a counted string in the target and set the target to the found
		 * range. Text is counted so it can contain NULs.
		 * Returns start of found range or -1 for failure in which case target is not moved.
		 */
		int64_t SearchInTargetFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		int64_t SearchInTarget(int64_t length, hstring const &text);

		/**
		 * Show a call tip containing a definition near position pos.
		 */
		void CallTipShowFromBuffer(int64_t pos, Windows::Storage::Streams::IBuffer const &definition);
		void CallTipShow(int64_t pos, hstring const &definition);

		/**
		 * Remove the call tip from the screen.
		 */
		void CallTipCancel();

		/**
		 * Is there an active call tip?
		 */
		bool CallTipActive();

		/**
		 * Retrieve the position where the caret was before displaying the call tip.
		 */
		int64_t CallTipPosStart();

		/**
		 * Highlight a segment of the definition.
		 */
		void CallTipSetHlt(int64_t highlightStart, int64_t highlightEnd);

		/**
		 * Find the display line of a document line taking hidden lines into account.
		 */
		int64_t VisibleFromDocLine(int64_t docLine);

		/**
		 * Find the document line of a display line taking hidden lines into account.
		 */
		int64_t DocLineFromVisible(int64_t displayLine);

		/**
		 * The number of display lines needed to wrap a document line
		 */
		int64_t WrapCount(int64_t docLine);

		/**
		 * Make a range of lines visible.
		 */
		void ShowLines(int64_t lineStart, int64_t lineEnd);

		/**
		 * Make a range of lines invisible.
		 */
		void HideLines(int64_t lineStart, int64_t lineEnd);

		/**
		 * Switch a header line between expanded and contracted.
		 */
		void ToggleFold(int64_t line);

		/**
		 * Switch a header line between expanded and contracted and show some text after the line.
		 */
		void ToggleFoldShowTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		void ToggleFoldShowText(int64_t line, hstring const &text);

		/**
		 * Set the default fold display text.
		 */
		void SetDefaultFoldDisplayTextFromBuffer(Windows::Storage::Streams::IBuffer const &text);
		void SetDefaultFoldDisplayText(hstring const &text);

		/**
		 * Get the default fold display text.
		 */
		int32_t GetDefaultFoldDisplayTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text);
		hstring GetDefaultFoldDisplayText();

		/**
		 * Expand or contract a fold header.
		 */
		void FoldLine(int64_t line, WinUIEditor::FoldAction const &action);

		/**
		 * Expand or contract a fold header and its children.
		 */
		void FoldChildren(int64_t line, WinUIEditor::FoldAction const &action);

		/**
		 * Expand a fold header and all children. Use the level argument instead of the line's current level.
		 */
		void ExpandChildren(int64_t line, WinUIEditor::FoldLevel const &level);

		/**
		 * Expand or contract all fold headers.
		 */
		void FoldAll(WinUIEditor::FoldAction const &action);

		/**
		 * Ensure a particular line is visible by expanding any header line hiding it.
		 */
		void EnsureVisible(int64_t line);

		/**
		 * Ensure a particular line is visible by expanding any header line hiding it.
		 * Use the currently set visibility policy to determine which range to display.
		 */
		void EnsureVisibleEnforcePolicy(int64_t line);

		/**
		 * Get position of start of word.
		 */
		int64_t WordStartPosition(int64_t pos, bool onlyWordCharacters);

		/**
		 * Get position of end of word.
		 */
		int64_t WordEndPosition(int64_t pos, bool onlyWordCharacters);

		/**
		 * Is the range start..end considered a word?
		 */
		bool IsRangeWord(int64_t start, int64_t end);

		/**
		 * Measure the pixel width of some text in a particular style.
		 * NUL terminated text argument.
		 * Does not handle tab or control characters.
		 */
		int32_t TextWidthFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &text);
		int32_t TextWidth(int32_t style, hstring const &text);

		/**
		 * Retrieve the height of a particular line of text in pixels.
		 */
		int32_t TextHeight(int64_t line);

		/**
		 * Append a string to the end of the document without changing the selection.
		 */
		void AppendTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		void AppendText(int64_t length, hstring const &text);

		/**
		 * Join the lines in the target.
		 */
		void LinesJoin();

		/**
		 * Split the lines in the target into lines that are less wide than pixelWidth
		 * where possible.
		 */
		void LinesSplit(int32_t pixelWidth);

		/**
		 * Set one of the colours used as a chequerboard pattern in the fold margin
		 */
		void SetFoldMarginColour(bool useSetting, int32_t back);

		/**
		 * Set the other colour used as a chequerboard pattern in the fold margin
		 */
		void SetFoldMarginHiColour(bool useSetting, int32_t fore);

		/**
		 * Move caret down one line.
		 */
		void LineDown();

		/**
		 * Move caret down one line extending selection to new caret position.
		 */
		void LineDownExtend();

		/**
		 * Move caret up one line.
		 */
		void LineUp();

		/**
		 * Move caret up one line extending selection to new caret position.
		 */
		void LineUpExtend();

		/**
		 * Move caret left one character.
		 */
		void CharLeft();

		/**
		 * Move caret left one character extending selection to new caret position.
		 */
		void CharLeftExtend();

		/**
		 * Move caret right one character.
		 */
		void CharRight();

		/**
		 * Move caret right one character extending selection to new caret position.
		 */
		void CharRightExtend();

		/**
		 * Move caret left one word.
		 */
		void WordLeft();

		/**
		 * Move caret left one word extending selection to new caret position.
		 */
		void WordLeftExtend();

		/**
		 * Move caret right one word.
		 */
		void WordRight();

		/**
		 * Move caret right one word extending selection to new caret position.
		 */
		void WordRightExtend();

		/**
		 * Move caret to first position on line.
		 */
		void Home();

		/**
		 * Move caret to first position on line extending selection to new caret position.
		 */
		void HomeExtend();

		/**
		 * Move caret to last position on line.
		 */
		void LineEnd();

		/**
		 * Move caret to last position on line extending selection to new caret position.
		 */
		void LineEndExtend();

		/**
		 * Move caret to first position in document.
		 */
		void DocumentStart();

		/**
		 * Move caret to first position in document extending selection to new caret position.
		 */
		void DocumentStartExtend();

		/**
		 * Move caret to last position in document.
		 */
		void DocumentEnd();

		/**
		 * Move caret to last position in document extending selection to new caret position.
		 */
		void DocumentEndExtend();

		/**
		 * Move caret one page up.
		 */
		void PageUp();

		/**
		 * Move caret one page up extending selection to new caret position.
		 */
		void PageUpExtend();

		/**
		 * Move caret one page down.
		 */
		void PageDown();

		/**
		 * Move caret one page down extending selection to new caret position.
		 */
		void PageDownExtend();

		/**
		 * Switch from insert to overtype mode or the reverse.
		 */
		void EditToggleOvertype();

		/**
		 * Cancel any modes such as call tip or auto-completion list display.
		 */
		void Cancel();

		/**
		 * Delete the selection or if no selection, the character before the caret.
		 */
		void DeleteBack();

		/**
		 * If selection is empty or all on one line replace the selection with a tab character.
		 * If more than one line selected, indent the lines.
		 */
		void Tab();

		/**
		 * Indent the current and selected lines.
		 */
		void LineIndent();

		/**
		 * If selection is empty or all on one line dedent the line if caret is at start, else move caret.
		 * If more than one line selected, dedent the lines.
		 */
		void BackTab();

		/**
		 * Dedent the current and selected lines.
		 */
		void LineDedent();

		/**
		 * Insert a new line, may use a CRLF, CR or LF depending on EOL mode.
		 */
		void NewLine();

		/**
		 * Insert a Form Feed character.
		 */
		void FormFeed();

		/**
		 * Move caret to before first visible character on line.
		 * If already there move to first character on line.
		 */
		void VCHome();

		/**
		 * Like VCHome but extending selection to new caret position.
		 */
		void VCHomeExtend();

		/**
		 * Magnify the displayed text by increasing the sizes by 1 point.
		 */
		void ZoomIn();

		/**
		 * Make the displayed text smaller by decreasing the sizes by 1 point.
		 */
		void ZoomOut();

		/**
		 * Delete the word to the left of the caret.
		 */
		void DelWordLeft();

		/**
		 * Delete the word to the right of the caret.
		 */
		void DelWordRight();

		/**
		 * Delete the word to the right of the caret, but not the trailing non-word characters.
		 */
		void DelWordRightEnd();

		/**
		 * Cut the line containing the caret.
		 */
		void LineCut();

		/**
		 * Delete the line containing the caret.
		 */
		void LineDelete();

		/**
		 * Switch the current line with the previous.
		 */
		void LineTranspose();

		/**
		 * Reverse order of selected lines.
		 */
		void LineReverse();

		/**
		 * Duplicate the current line.
		 */
		void LineDuplicate();

		/**
		 * Transform the selection to lower case.
		 */
		void LowerCase();

		/**
		 * Transform the selection to upper case.
		 */
		void UpperCase();

		/**
		 * Scroll the document down, keeping the caret visible.
		 */
		void LineScrollDown();

		/**
		 * Scroll the document up, keeping the caret visible.
		 */
		void LineScrollUp();

		/**
		 * Delete the selection or if no selection, the character before the caret.
		 * Will not delete the character before at the start of a line.
		 */
		void DeleteBackNotLine();

		/**
		 * Move caret to first position on display line.
		 */
		void HomeDisplay();

		/**
		 * Move caret to first position on display line extending selection to
		 * new caret position.
		 */
		void HomeDisplayExtend();

		/**
		 * Move caret to last position on display line.
		 */
		void LineEndDisplay();

		/**
		 * Move caret to last position on display line extending selection to new
		 * caret position.
		 */
		void LineEndDisplayExtend();

		/**
		 * Like Home but when word-wrap is enabled goes first to start of display line
		 * HomeDisplay, then to start of document line Home.
		 */
		void HomeWrap();

		/**
		 * Like HomeExtend but when word-wrap is enabled extends first to start of display line
		 * HomeDisplayExtend, then to start of document line HomeExtend.
		 */
		void HomeWrapExtend();

		/**
		 * Like LineEnd but when word-wrap is enabled goes first to end of display line
		 * LineEndDisplay, then to start of document line LineEnd.
		 */
		void LineEndWrap();

		/**
		 * Like LineEndExtend but when word-wrap is enabled extends first to end of display line
		 * LineEndDisplayExtend, then to start of document line LineEndExtend.
		 */
		void LineEndWrapExtend();

		/**
		 * Like VCHome but when word-wrap is enabled goes first to start of display line
		 * VCHomeDisplay, then behaves like VCHome.
		 */
		void VCHomeWrap();

		/**
		 * Like VCHomeExtend but when word-wrap is enabled extends first to start of display line
		 * VCHomeDisplayExtend, then behaves like VCHomeExtend.
		 */
		void VCHomeWrapExtend();

		/**
		 * Copy the line containing the caret.
		 */
		void LineCopy();

		/**
		 * Move the caret inside current view if it's not there already.
		 */
		void MoveCaretInsideView();

		/**
		 * How many characters are on a line, including end of line characters?
		 */
		int64_t LineLength(int64_t line);

		/**
		 * Highlight the characters at two positions.
		 */
		void BraceHighlight(int64_t posA, int64_t posB);

		/**
		 * Use specified indicator to highlight matching braces instead of changing their style.
		 */
		void BraceHighlightIndicator(bool useSetting, int32_t indicator);

		/**
		 * Highlight the character at a position indicating there is no matching brace.
		 */
		void BraceBadLight(int64_t pos);

		/**
		 * Use specified indicator to highlight non matching brace instead of changing its style.
		 */
		void BraceBadLightIndicator(bool useSetting, int32_t indicator);

		/**
		 * Find the position of a matching brace or INVALID_POSITION if no match.
		 * The maxReStyle must be 0 for now. It may be defined in a future release.
		 */
		int64_t BraceMatch(int64_t pos, int32_t maxReStyle);

		/**
		 * Similar to BraceMatch, but matching starts at the explicit start position.
		 */
		int64_t BraceMatchNext(int64_t pos, int64_t startPos);

		/**
		 * Add a new vertical edge to the view.
		 */
		void MultiEdgeAddLine(int64_t column, int32_t edgeColour);

		/**
		 * Clear all vertical edges.
		 */
		void MultiEdgeClearAll();

		/**
		 * Sets the current caret position to be the search anchor.
		 */
		void SearchAnchor();

		/**
		 * Find some text starting at the search anchor.
		 * Does not ensure the selection is visible.
		 */
		int64_t SearchNextFromBuffer(WinUIEditor::FindOption const &searchFlags, Windows::Storage::Streams::IBuffer const &text);
		int64_t SearchNext(WinUIEditor::FindOption const &searchFlags, hstring const &text);

		/**
		 * Find some text starting at the search anchor and moving backwards.
		 * Does not ensure the selection is visible.
		 */
		int64_t SearchPrevFromBuffer(WinUIEditor::FindOption const &searchFlags, Windows::Storage::Streams::IBuffer const &text);
		int64_t SearchPrev(WinUIEditor::FindOption const &searchFlags, hstring const &text);

		/**
		 * Set whether a pop up menu is displayed automatically when the user presses
		 * the wrong mouse button on certain areas.
		 */
		void UsePopUp(WinUIEditor::PopUp const &popUpMode);

		/**
		 * Create a new document object.
		 * Starts with reference count of 1 and not selected into editor.
		 */
		uint64_t CreateDocument(int64_t bytes, WinUIEditor::DocumentOption const &documentOptions);

		/**
		 * Extend life of document.
		 */
		void AddRefDocument(uint64_t doc);

		/**
		 * Release a reference to the document, deleting document if it fades to black.
		 */
		void ReleaseDocument(uint64_t doc);

		/**
		 * Move to the previous change in capitalisation.
		 */
		void WordPartLeft();

		/**
		 * Move to the previous change in capitalisation extending selection
		 * to new caret position.
		 */
		void WordPartLeftExtend();

		/**
		 * Move to the change next in capitalisation.
		 */
		void WordPartRight();

		/**
		 * Move to the next change in capitalisation extending selection
		 * to new caret position.
		 */
		void WordPartRightExtend();

		/**
		 * Set the way the display area is determined when a particular line
		 * is to be moved to by Find, FindNext, GotoLine, etc.
		 */
		void SetVisiblePolicy(WinUIEditor::VisiblePolicy const &visiblePolicy, int32_t visibleSlop);

		/**
		 * Delete back from the current position to the start of the line.
		 */
		void DelLineLeft();

		/**
		 * Delete forwards from the current position to the end of the line.
		 */
		void DelLineRight();

		/**
		 * Set the last x chosen value to be the caret x position.
		 */
		void ChooseCaretX();

		/**
		 * Set the focus to this Scintilla widget.
		 */
		void GrabFocus();

		/**
		 * Set the way the caret is kept visible when going sideways.
		 * The exclusion zone is given in pixels.
		 */
		void SetXCaretPolicy(WinUIEditor::CaretPolicy const &caretPolicy, int32_t caretSlop);

		/**
		 * Set the way the line the caret is on is kept visible.
		 * The exclusion zone is given in lines.
		 */
		void SetYCaretPolicy(WinUIEditor::CaretPolicy const &caretPolicy, int32_t caretSlop);

		/**
		 * Move caret down one paragraph (delimited by empty lines).
		 */
		void ParaDown();

		/**
		 * Extend selection down one paragraph (delimited by empty lines).
		 */
		void ParaDownExtend();

		/**
		 * Move caret up one paragraph (delimited by empty lines).
		 */
		void ParaUp();

		/**
		 * Extend selection up one paragraph (delimited by empty lines).
		 */
		void ParaUpExtend();

		/**
		 * Given a valid document position, return the previous position taking code
		 * page into account. Returns 0 if passed 0.
		 */
		int64_t PositionBefore(int64_t pos);

		/**
		 * Given a valid document position, return the next position taking code
		 * page into account. Maximum value returned is the last position in the document.
		 */
		int64_t PositionAfter(int64_t pos);

		/**
		 * Given a valid document position, return a position that differs in a number
		 * of characters. Returned value is always between 0 and last position in document.
		 */
		int64_t PositionRelative(int64_t pos, int64_t relative);

		/**
		 * Given a valid document position, return a position that differs in a number
		 * of UTF-16 code units. Returned value is always between 0 and last position in document.
		 * The result may point half way (2 bytes) inside a non-BMP character.
		 */
		int64_t PositionRelativeCodeUnits(int64_t pos, int64_t relative);

		/**
		 * Copy a range of text to the clipboard. Positions are clipped into the document.
		 */
		void CopyRange(int64_t start, int64_t end);

		/**
		 * Copy argument text to the clipboard.
		 */
		void CopyTextFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		void CopyText(int64_t length, hstring const &text);

		/**
		 * Set the selection mode to stream (SC_SEL_STREAM) or rectangular (SC_SEL_RECTANGLE/SC_SEL_THIN) or
		 * by lines (SC_SEL_LINES) without changing MoveExtendsSelection.
		 */
		void ChangeSelectionMode(WinUIEditor::SelectionMode const &selectionMode);

		/**
		 * Retrieve the position of the start of the selection at the given line (INVALID_POSITION if no selection on this line).
		 */
		int64_t GetLineSelStartPosition(int64_t line);

		/**
		 * Retrieve the position of the end of the selection at the given line (INVALID_POSITION if no selection on this line).
		 */
		int64_t GetLineSelEndPosition(int64_t line);

		/**
		 * Move caret down one line, extending rectangular selection to new caret position.
		 */
		void LineDownRectExtend();

		/**
		 * Move caret up one line, extending rectangular selection to new caret position.
		 */
		void LineUpRectExtend();

		/**
		 * Move caret left one character, extending rectangular selection to new caret position.
		 */
		void CharLeftRectExtend();

		/**
		 * Move caret right one character, extending rectangular selection to new caret position.
		 */
		void CharRightRectExtend();

		/**
		 * Move caret to first position on line, extending rectangular selection to new caret position.
		 */
		void HomeRectExtend();

		/**
		 * Move caret to before first visible character on line.
		 * If already there move to first character on line.
		 * In either case, extend rectangular selection to new caret position.
		 */
		void VCHomeRectExtend();

		/**
		 * Move caret to last position on line, extending rectangular selection to new caret position.
		 */
		void LineEndRectExtend();

		/**
		 * Move caret one page up, extending rectangular selection to new caret position.
		 */
		void PageUpRectExtend();

		/**
		 * Move caret one page down, extending rectangular selection to new caret position.
		 */
		void PageDownRectExtend();

		/**
		 * Move caret to top of page, or one page up if already at top of page.
		 */
		void StutteredPageUp();

		/**
		 * Move caret to top of page, or one page up if already at top of page, extending selection to new caret position.
		 */
		void StutteredPageUpExtend();

		/**
		 * Move caret to bottom of page, or one page down if already at bottom of page.
		 */
		void StutteredPageDown();

		/**
		 * Move caret to bottom of page, or one page down if already at bottom of page, extending selection to new caret position.
		 */
		void StutteredPageDownExtend();

		/**
		 * Move caret left one word, position cursor at end of word.
		 */
		void WordLeftEnd();

		/**
		 * Move caret left one word, position cursor at end of word, extending selection to new caret position.
		 */
		void WordLeftEndExtend();

		/**
		 * Move caret right one word, position cursor at end of word.
		 */
		void WordRightEnd();

		/**
		 * Move caret right one word, position cursor at end of word, extending selection to new caret position.
		 */
		void WordRightEndExtend();

		/**
		 * Reset the set of characters for whitespace and word characters to the defaults.
		 */
		void SetCharsDefault();

		/**
		 * Enlarge the document to a particular size of text bytes.
		 */
		void Allocate(int64_t bytes);

		/**
		 * Returns the target converted to UTF8.
		 * Return the length in bytes.
		 */
		int64_t TargetAsUTF8WriteBuffer(Windows::Storage::Streams::IBuffer const &s);
		hstring TargetAsUTF8();

		/**
		 * Set the length of the utf8 argument for calling EncodedFromUTF8.
		 * Set to -1 and the string will be measured to the first nul.
		 */
		void SetLengthForEncode(int64_t bytes);

		/**
		 * Translates a UTF8 string into the document encoding.
		 * Return the length of the result in bytes.
		 * On error return 0.
		 */
		int64_t EncodedFromUTF8WriteBuffer(Windows::Storage::Streams::IBuffer const &utf8, Windows::Storage::Streams::IBuffer const &encoded);
		hstring EncodedFromUTF8(hstring const &utf8);

		/**
		 * Find the position of a column on a line taking into account tabs and
		 * multi-byte characters. If beyond end of line, return line end position.
		 */
		int64_t FindColumn(int64_t line, int64_t column);

		/**
		 * Switch between sticky and non-sticky: meant to be bound to a key.
		 */
		void ToggleCaretSticky();

		/**
		 * Replace the selection with text like a rectangular paste.
		 */
		void ReplaceRectangularFromBuffer(int64_t length, Windows::Storage::Streams::IBuffer const &text);
		void ReplaceRectangular(int64_t length, hstring const &text);

		/**
		 * Duplicate the selection. If selection empty duplicate the line containing the caret.
		 */
		void SelectionDuplicate();

		/**
		 * Turn a indicator on over a range.
		 */
		void IndicatorFillRange(int64_t start, int64_t lengthFill);

		/**
		 * Turn a indicator off over a range.
		 */
		void IndicatorClearRange(int64_t start, int64_t lengthClear);

		/**
		 * Are any indicators present at pos?
		 */
		int32_t IndicatorAllOnFor(int64_t pos);

		/**
		 * What value does a particular indicator have at a position?
		 */
		int32_t IndicatorValueAt(int32_t indicator, int64_t pos);

		/**
		 * Where does a particular indicator start?
		 */
		int64_t IndicatorStart(int32_t indicator, int64_t pos);

		/**
		 * Where does a particular indicator end?
		 */
		int64_t IndicatorEnd(int32_t indicator, int64_t pos);

		/**
		 * Copy the selection, if selection empty copy the line with the caret
		 */
		void CopyAllowLine();

		/**
		 * Cut the selection, if selection empty cut the line with the caret
		 */
		void CutAllowLine();

		/**
		 * Which symbol was defined for markerNumber with MarkerDefine
		 */
		WinUIEditor::MarkerSymbol MarkerSymbolDefined(int32_t markerNumber);

		/**
		 * Clear the margin text on all lines
		 */
		void MarginTextClearAll();

		/**
		 * Clear the annotations from all lines
		 */
		void AnnotationClearAll();

		/**
		 * Release all extended (>255) style numbers
		 */
		void ReleaseAllExtendedStyles();

		/**
		 * Allocate some extended (>255) style numbers and return the start of the range
		 */
		int32_t AllocateExtendedStyles(int32_t numberStyles);

		/**
		 * Add a container action to the undo stack
		 */
		void AddUndoAction(int32_t token, WinUIEditor::UndoFlags const &flags);

		/**
		 * Find the position of a character from a point within the window.
		 */
		int64_t CharPositionFromPoint(int32_t x, int32_t y);

		/**
		 * Find the position of a character from a point within the window.
		 * Return INVALID_POSITION if not close to text.
		 */
		int64_t CharPositionFromPointClose(int32_t x, int32_t y);

		/**
		 * Clear selections to a single empty stream selection
		 */
		void ClearSelections();

		/**
		 * Set a simple selection
		 */
		void SetSelection(int64_t caret, int64_t anchor);

		/**
		 * Add a selection
		 */
		void AddSelection(int64_t caret, int64_t anchor);

		/**
		 * Find the selection index for a point. -1 when not at a selection.
		 */
		int32_t SelectionFromPoint(int32_t x, int32_t y);

		/**
		 * Drop one selection
		 */
		void DropSelectionN(int32_t selection);

		/**
		 * Set the main selection to the next selection.
		 */
		void RotateSelection();

		/**
		 * Swap that caret and anchor of the main selection.
		 */
		void SwapMainAnchorCaret();

		/**
		 * Add the next occurrence of the main selection to the set of selections as main.
		 * If the current selection is empty then select word around caret.
		 */
		void MultipleSelectAddNext();

		/**
		 * Add each occurrence of the main selection in the target to the set of selections.
		 * If the current selection is empty then select word around caret.
		 */
		void MultipleSelectAddEach();

		/**
		 * Indicate that the internal state of a lexer has changed over a range and therefore
		 * there may be a need to redraw.
		 */
		int32_t ChangeLexerState(int64_t start, int64_t end);

		/**
		 * Find the next line at or after lineStart that is a contracted fold header line.
		 * Return -1 when no more lines.
		 */
		int64_t ContractedFoldNext(int64_t lineStart);

		/**
		 * Centre current line in window.
		 */
		void VerticalCentreCaret();

		/**
		 * Move the selected lines up one line, shifting the line above after the selection
		 */
		void MoveSelectedLinesUp();

		/**
		 * Move the selected lines down one line, shifting the line below before the selection
		 */
		void MoveSelectedLinesDown();

		/**
		 * Define a marker from RGBA data.
		 * It has the width and height from RGBAImageSetWidth/Height
		 */
		void MarkerDefineRGBAImageFromBuffer(int32_t markerNumber, Windows::Storage::Streams::IBuffer const &pixels);
		void MarkerDefineRGBAImage(int32_t markerNumber, hstring const &pixels);

		/**
		 * Register an RGBA image for use in autocompletion lists.
		 * It has the width and height from RGBAImageSetWidth/Height
		 */
		void RegisterRGBAImageFromBuffer(int32_t type, Windows::Storage::Streams::IBuffer const &pixels);
		void RegisterRGBAImage(int32_t type, hstring const &pixels);

		/**
		 * Scroll to start of document.
		 */
		void ScrollToStart();

		/**
		 * Scroll to end of document.
		 */
		void ScrollToEnd();

		/**
		 * Create an ILoader*.
		 */
		uint64_t CreateLoader(int64_t bytes, WinUIEditor::DocumentOption const &documentOptions);

		/**
		 * On macOS, show a find indicator.
		 */
		void FindIndicatorShow(int64_t start, int64_t end);

		/**
		 * On macOS, flash a find indicator, then fade out.
		 */
		void FindIndicatorFlash(int64_t start, int64_t end);

		/**
		 * On macOS, hide the find indicator.
		 */
		void FindIndicatorHide();

		/**
		 * Move caret to before first visible character on display line.
		 * If already there move to first character on display line.
		 */
		void VCHomeDisplay();

		/**
		 * Like VCHomeDisplay but extending selection to new caret position.
		 */
		void VCHomeDisplayExtend();

		/**
		 * Remove a character representation.
		 */
		void ClearRepresentationFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter);
		void ClearRepresentation(hstring const &encodedCharacter);

		/**
		 * Clear representations to default.
		 */
		void ClearAllRepresentations();

		/**
		 * Clear the end of annotations from all lines
		 */
		void EOLAnnotationClearAll();

		/**
		 * Request line character index be created or its use count increased.
		 */
		void AllocateLineCharacterIndex(WinUIEditor::LineCharacterIndexType const &lineCharacterIndex);

		/**
		 * Decrease use count of line character index and remove if 0.
		 */
		void ReleaseLineCharacterIndex(WinUIEditor::LineCharacterIndexType const &lineCharacterIndex);

		/**
		 * Retrieve the document line containing a position measured in index units.
		 */
		int64_t LineFromIndexPosition(int64_t pos, WinUIEditor::LineCharacterIndexType const &lineCharacterIndex);

		/**
		 * Retrieve the position measured in index units at the start of a document line.
		 */
		int64_t IndexPositionFromLine(int64_t line, WinUIEditor::LineCharacterIndexType const &lineCharacterIndex);

		/**
		 * Start notifying the container of all key presses and commands.
		 */
		void StartRecord();

		/**
		 * Stop notifying the container of all key presses and commands.
		 */
		void StopRecord();

		/**
		 * Colourise a segment of the document using the current lexing language.
		 */
		void Colourise(int64_t start, int64_t end);

		/**
		 * For private communication between an application and a known lexer.
		 */
		uint64_t PrivateLexerCall(int32_t operation, uint64_t pointer);

		/**
		 * Retrieve a '\n' separated list of properties understood by the current lexer.
		 * Result is NUL-terminated.
		 */
		int32_t PropertyNamesWriteBuffer(Windows::Storage::Streams::IBuffer const &names);
		hstring PropertyNames();

		/**
		 * Retrieve the type of a property.
		 */
		WinUIEditor::TypeProperty PropertyTypeFromBuffer(Windows::Storage::Streams::IBuffer const &name);
		WinUIEditor::TypeProperty PropertyType(hstring const &name);

		/**
		 * Describe a property.
		 * Result is NUL-terminated.
		 */
		int32_t DescribePropertyWriteBuffer(Windows::Storage::Streams::IBuffer const &name, Windows::Storage::Streams::IBuffer const &description);
		hstring DescribeProperty(hstring const &name);

		/**
		 * Retrieve a '\n' separated list of descriptions of the keyword sets understood by the current lexer.
		 * Result is NUL-terminated.
		 */
		int32_t DescribeKeyWordSetsWriteBuffer(Windows::Storage::Streams::IBuffer const &descriptions);
		hstring DescribeKeyWordSets();

		/**
		 * Allocate a set of sub styles for a particular base style, returning start of range
		 */
		int32_t AllocateSubStyles(int32_t styleBase, int32_t numberStyles);

		/**
		 * Free allocated sub styles
		 */
		void FreeSubStyles();

		/**
		 * Retrieve the name of a style.
		 * Result is NUL-terminated.
		 */
		int32_t NameOfStyleWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &name);
		hstring NameOfStyle(int32_t style);

		/**
		 * Retrieve a ' ' separated list of style tags like "literal quoted string".
		 * Result is NUL-terminated.
		 */
		int32_t TagsOfStyleWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &tags);
		hstring TagsOfStyle(int32_t style);

		/**
		 * Retrieve a description of a style.
		 * Result is NUL-terminated.
		 */
		int32_t DescriptionOfStyleWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &description);
		hstring DescriptionOfStyle(int32_t style);

		/**
		 * Returns the character byte at the position.
		 */
		int32_t GetCharAt(int64_t pos);

		/**
		 * Returns the style byte at the position.
		 */
		int32_t GetStyleAt(int64_t pos);

		/**
		 * Returns the unsigned style byte at the position.
		 */
		int32_t GetStyleIndexAt(int64_t pos);

		/**
		 * Get the locale for displaying text.
		 */
		int32_t GetFontLocaleWriteBuffer(Windows::Storage::Streams::IBuffer const &localeName);
		hstring GetFontLocale();

		/**
		 * Get the layer used for a marker that is drawn in the text area, not the margin.
		 */
		WinUIEditor::Layer MarkerGetLayer(int32_t markerNumber);

		/**
		 * Retrieve the type of a margin.
		 */
		WinUIEditor::MarginType GetMarginTypeN(int32_t margin);

		/**
		 * Retrieve the width of a margin in pixels.
		 */
		int32_t GetMarginWidthN(int32_t margin);

		/**
		 * Retrieve the marker mask of a margin.
		 */
		int32_t GetMarginMaskN(int32_t margin);

		/**
		 * Retrieve the mouse click sensitivity of a margin.
		 */
		bool GetMarginSensitiveN(int32_t margin);

		/**
		 * Retrieve the cursor shown in a margin.
		 */
		WinUIEditor::CursorShape GetMarginCursorN(int32_t margin);

		/**
		 * Retrieve the background colour of a margin
		 */
		int32_t GetMarginBackN(int32_t margin);

		/**
		 * Get the foreground colour of a style.
		 */
		int32_t StyleGetFore(int32_t style);

		/**
		 * Get the background colour of a style.
		 */
		int32_t StyleGetBack(int32_t style);

		/**
		 * Get is a style bold or not.
		 */
		bool StyleGetBold(int32_t style);

		/**
		 * Get is a style italic or not.
		 */
		bool StyleGetItalic(int32_t style);

		/**
		 * Get the size of characters of a style.
		 */
		int32_t StyleGetSize(int32_t style);

		/**
		 * Get the font of a style.
		 * Returns the length of the fontName
		 * Result is NUL-terminated.
		 */
		int32_t StyleGetFontWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &fontName);
		hstring StyleGetFont(int32_t style);

		/**
		 * Get is a style to have its end of line filled or not.
		 */
		bool StyleGetEOLFilled(int32_t style);

		/**
		 * Get is a style underlined or not.
		 */
		bool StyleGetUnderline(int32_t style);

		/**
		 * Get is a style mixed case, or to force upper or lower case.
		 */
		WinUIEditor::CaseVisible StyleGetCase(int32_t style);

		/**
		 * Get the character get of the font in a style.
		 */
		WinUIEditor::CharacterSet StyleGetCharacterSet(int32_t style);

		/**
		 * Get is a style visible or not.
		 */
		bool StyleGetVisible(int32_t style);

		/**
		 * Get is a style changeable or not (read only).
		 * Experimental feature, currently buggy.
		 */
		bool StyleGetChangeable(int32_t style);

		/**
		 * Get is a style a hotspot or not.
		 */
		bool StyleGetHotSpot(int32_t style);

		/**
		 * Get the size of characters of a style in points multiplied by 100
		 */
		int32_t StyleGetSizeFractional(int32_t style);

		/**
		 * Get the weight of characters of a style.
		 */
		WinUIEditor::FontWeight StyleGetWeight(int32_t style);

		/**
		 * Get whether a style may be monospaced.
		 */
		bool StyleGetCheckMonospaced(int32_t style);

		/**
		 * Get the stretch of characters of a style.
		 */
		WinUIEditor::FontStretch StyleGetStretch(int32_t style);

		/**
		 * Get the invisible representation for a style.
		 */
		int32_t StyleGetInvisibleRepresentationWriteBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &representation);
		hstring StyleGetInvisibleRepresentation(int32_t style);

		/**
		 * Get the colour of an element.
		 */
		int32_t GetElementColour(WinUIEditor::Element const &element);

		/**
		 * Get whether an element has been set by SetElementColour.
		 * When false, a platform-defined or default colour is used.
		 */
		bool GetElementIsSet(WinUIEditor::Element const &element);

		/**
		 * Get whether an element supports translucency.
		 */
		bool GetElementAllowsTranslucent(WinUIEditor::Element const &element);

		/**
		 * Get the colour of an element.
		 */
		int32_t GetElementBaseColour(WinUIEditor::Element const &element);

		/**
		 * Get the set of characters making up words for when moving or selecting by word.
		 * Returns the number of characters
		 */
		int32_t GetWordCharsWriteBuffer(Windows::Storage::Streams::IBuffer const &characters);
		hstring GetWordChars();

		/**
		 * What is the type of an action?
		 */
		int32_t GetUndoActionType(int32_t action);

		/**
		 * What is the position of an action?
		 */
		int64_t GetUndoActionPosition(int32_t action);

		/**
		 * What is the text of an action?
		 */
		int32_t GetUndoActionTextWriteBuffer(int32_t action, Windows::Storage::Streams::IBuffer const &text);
		hstring GetUndoActionText(int32_t action);

		/**
		 * Retrieve the style of an indicator.
		 */
		WinUIEditor::IndicatorStyle IndicGetStyle(int32_t indicator);

		/**
		 * Retrieve the foreground colour of an indicator.
		 */
		int32_t IndicGetFore(int32_t indicator);

		/**
		 * Retrieve whether indicator drawn under or over text.
		 */
		bool IndicGetUnder(int32_t indicator);

		/**
		 * Retrieve the hover style of an indicator.
		 */
		WinUIEditor::IndicatorStyle IndicGetHoverStyle(int32_t indicator);

		/**
		 * Retrieve the foreground hover colour of an indicator.
		 */
		int32_t IndicGetHoverFore(int32_t indicator);

		/**
		 * Retrieve the attributes of an indicator.
		 */
		WinUIEditor::IndicFlag IndicGetFlags(int32_t indicator);

		/**
		 * Retrieve the stroke width of an indicator.
		 */
		int32_t IndicGetStrokeWidth(int32_t indicator);

		/**
		 * Retrieve the extra styling information for a line.
		 */
		int32_t GetLineState(int64_t line);

		/**
		 * Retrieve the number of columns that a line is indented.
		 */
		int32_t GetLineIndentation(int64_t line);

		/**
		 * Retrieve the position before the first non indentation character on a line.
		 */
		int64_t GetLineIndentPosition(int64_t line);

		/**
		 * Retrieve the column number of a position, taking tab width into account.
		 */
		int64_t GetColumn(int64_t pos);

		/**
		 * Get the position after the last visible characters on a line.
		 */
		int64_t GetLineEndPosition(int64_t line);

		/**
		 * Retrieve serialized form of selection.
		 */
		int64_t GetSelectionSerializedWriteBuffer(Windows::Storage::Streams::IBuffer const &selectionString);
		hstring GetSelectionSerialized();

		/**
		 * Retrieve the text in the target.
		 */
		int64_t GetTargetTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text);
		hstring GetTargetText();

		/**
		 * Retrieve the fold level of a line.
		 */
		WinUIEditor::FoldLevel GetFoldLevel(int64_t line);

		/**
		 * Find the last child line of a header line.
		 */
		int64_t GetLastChild(int64_t line, WinUIEditor::FoldLevel const &level);

		/**
		 * Find the parent line of a child line.
		 */
		int64_t GetFoldParent(int64_t line);

		/**
		 * Is a line visible?
		 */
		bool GetLineVisible(int64_t line);

		/**
		 * Is a header line expanded?
		 */
		bool GetFoldExpanded(int64_t line);

		/**
		 * Retrieve the value of a tag from a regular expression search.
		 * Result is NUL-terminated.
		 */
		int32_t GetTagWriteBuffer(int32_t tagNumber, Windows::Storage::Streams::IBuffer const &tagValue);
		hstring GetTag(int32_t tagNumber);

		/**
		 * Get multi edge positions.
		 */
		int64_t GetMultiEdgeColumn(int32_t which);

		/**
		 * Get the fore colour for active hotspots.
		 */
		int32_t GetHotspotActiveFore();

		/**
		 * Get the back colour for active hotspots.
		 */
		int32_t GetHotspotActiveBack();

		/**
		 * Get the set of characters making up whitespace for when moving or selecting by word.
		 */
		int32_t GetWhitespaceCharsWriteBuffer(Windows::Storage::Streams::IBuffer const &characters);
		hstring GetWhitespaceChars();

		/**
		 * Get the set of characters making up punctuation characters
		 */
		int32_t GetPunctuationCharsWriteBuffer(Windows::Storage::Streams::IBuffer const &characters);
		hstring GetPunctuationChars();

		/**
		 * Get currently selected item text in the auto-completion list
		 * Returns the length of the item text
		 * Result is NUL-terminated.
		 */
		int32_t AutoCGetCurrentTextWriteBuffer(Windows::Storage::Streams::IBuffer const &text);
		hstring AutoCGetCurrentText();

		/**
		 * Get the string to separate parts when copying a multiple selection.
		 */
		int32_t GetCopySeparatorWriteBuffer(Windows::Storage::Streams::IBuffer const &separator);
		hstring GetCopySeparator();

		/**
		 * Return a read-only pointer to a range of characters in the document.
		 * May move the gap so that the range is contiguous, but will only move up
		 * to lengthRange bytes.
		 */
		uint64_t GetRangePointer(int64_t start, int64_t lengthRange);

		/**
		 * Get the alpha fill colour of the given indicator.
		 */
		WinUIEditor::Alpha IndicGetAlpha(int32_t indicator);

		/**
		 * Get the alpha outline colour of the given indicator.
		 */
		WinUIEditor::Alpha IndicGetOutlineAlpha(int32_t indicator);

		/**
		 * Get the text in the text margin for a line
		 */
		int32_t MarginGetTextWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		hstring MarginGetText(int64_t line);

		/**
		 * Get the style number for the text margin for a line
		 */
		int32_t MarginGetStyle(int64_t line);

		/**
		 * Get the styles in the text margin for a line
		 */
		int32_t MarginGetStylesWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles);
		hstring MarginGetStyles(int64_t line);

		/**
		 * Get the annotation text for a line
		 */
		int32_t AnnotationGetTextWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		hstring AnnotationGetText(int64_t line);

		/**
		 * Get the style number for the annotations for a line
		 */
		int32_t AnnotationGetStyle(int64_t line);

		/**
		 * Get the annotation styles for a line
		 */
		int32_t AnnotationGetStylesWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles);
		hstring AnnotationGetStyles(int64_t line);

		/**
		 * Get the number of annotation lines for a line
		 */
		int32_t AnnotationGetLines(int64_t line);

		/**
		 * Return the caret position of the nth selection.
		 */
		int64_t GetSelectionNCaret(int32_t selection);

		/**
		 * Return the anchor position of the nth selection.
		 */
		int64_t GetSelectionNAnchor(int32_t selection);

		/**
		 * Return the virtual space of the caret of the nth selection.
		 */
		int64_t GetSelectionNCaretVirtualSpace(int32_t selection);

		/**
		 * Return the virtual space of the anchor of the nth selection.
		 */
		int64_t GetSelectionNAnchorVirtualSpace(int32_t selection);

		/**
		 * Returns the position at the start of the selection.
		 */
		int64_t GetSelectionNStart(int32_t selection);

		/**
		 * Returns the virtual space at the start of the selection.
		 */
		int64_t GetSelectionNStartVirtualSpace(int32_t selection);

		/**
		 * Returns the virtual space at the end of the selection.
		 */
		int64_t GetSelectionNEndVirtualSpace(int32_t selection);

		/**
		 * Returns the position at the end of the selection.
		 */
		int64_t GetSelectionNEnd(int32_t selection);

		/**
		 * Get the way a character is drawn.
		 * Result is NUL-terminated.
		 */
		int32_t GetRepresentationWriteBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, Windows::Storage::Streams::IBuffer const &representation);
		hstring GetRepresentation(hstring const &encodedCharacter);

		/**
		 * Get the appearance of a representation.
		 */
		WinUIEditor::RepresentationAppearance GetRepresentationAppearanceFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter);
		WinUIEditor::RepresentationAppearance GetRepresentationAppearance(hstring const &encodedCharacter);

		/**
		 * Get the colour of a representation.
		 */
		int32_t GetRepresentationColourFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter);
		int32_t GetRepresentationColour(hstring const &encodedCharacter);

		/**
		 * Get the end of line annotation text for a line
		 */
		int32_t EOLAnnotationGetTextWriteBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		hstring EOLAnnotationGetText(int64_t line);

		/**
		 * Get the style number for the end of line annotations for a line
		 */
		int32_t EOLAnnotationGetStyle(int64_t line);

		/**
		 * Get whether a feature is supported
		 */
		bool SupportsFeature(WinUIEditor::Supports const &feature);

		/**
		 * Retrieve a "property" value previously set with SetProperty.
		 * Result is NUL-terminated.
		 */
		int32_t GetPropertyWriteBuffer(Windows::Storage::Streams::IBuffer const &key, Windows::Storage::Streams::IBuffer const &value);
		hstring GetProperty(hstring const &key);

		/**
		 * Retrieve a "property" value previously set with SetProperty,
		 * with "$()" variable replacement on returned buffer.
		 * Result is NUL-terminated.
		 */
		int32_t GetPropertyExpandedWriteBuffer(Windows::Storage::Streams::IBuffer const &key, Windows::Storage::Streams::IBuffer const &value);
		hstring GetPropertyExpanded(hstring const &key);

		/**
		 * Retrieve a "property" value previously set with SetProperty,
		 * interpreted as an int AFTER any "$()" variable replacement.
		 */
		int32_t GetPropertyIntFromBuffer(Windows::Storage::Streams::IBuffer const &key, int32_t defaultValue);
		int32_t GetPropertyInt(hstring const &key, int32_t defaultValue);

		/**
		 * Retrieve the name of the lexer.
		 * Return the length of the text.
		 * Result is NUL-terminated.
		 */
		int32_t GetLexerLanguageWriteBuffer(Windows::Storage::Streams::IBuffer const &language);
		hstring GetLexerLanguage();

		/**
		 * The starting style number for the sub styles associated with a base style
		 */
		int32_t GetSubStylesStart(int32_t styleBase);

		/**
		 * The number of sub styles associated with a base style
		 */
		int32_t GetSubStylesLength(int32_t styleBase);

		/**
		 * For a sub style, return the base style, else return the argument.
		 */
		int32_t GetStyleFromSubStyle(int32_t subStyle);

		/**
		 * For a secondary style, return the primary style, else return the argument.
		 */
		int32_t GetPrimaryStyleFromStyle(int32_t style);

		/**
		 * Get the set of base styles that can be extended with sub styles
		 * Result is NUL-terminated.
		 */
		int32_t GetSubStyleBasesWriteBuffer(Windows::Storage::Streams::IBuffer const &styles);
		hstring GetSubStyleBases();

		/**
		 * Set the locale for displaying text.
		 */
		void SetFontLocaleFromBuffer(Windows::Storage::Streams::IBuffer const &localeName);
		void SetFontLocale(hstring const &localeName);

		/**
		 * Set the foreground colour used for a particular marker number.
		 */
		void MarkerSetFore(int32_t markerNumber, int32_t fore);

		/**
		 * Set the background colour used for a particular marker number.
		 */
		void MarkerSetBack(int32_t markerNumber, int32_t back);

		/**
		 * Set the background colour used for a particular marker number when its folding block is selected.
		 */
		void MarkerSetBackSelected(int32_t markerNumber, int32_t back);

		/**
		 * Set the foreground colour used for a particular marker number.
		 */
		void MarkerSetForeTranslucent(int32_t markerNumber, int32_t fore);

		/**
		 * Set the background colour used for a particular marker number.
		 */
		void MarkerSetBackTranslucent(int32_t markerNumber, int32_t back);

		/**
		 * Set the background colour used for a particular marker number when its folding block is selected.
		 */
		void MarkerSetBackSelectedTranslucent(int32_t markerNumber, int32_t back);

		/**
		 * Set the width of strokes used in .01 pixels so 50  = 1/2 pixel width.
		 */
		void MarkerSetStrokeWidth(int32_t markerNumber, int32_t hundredths);

		/**
		 * Set the alpha used for a marker that is drawn in the text area, not the margin.
		 */
		void MarkerSetAlpha(int32_t markerNumber, WinUIEditor::Alpha const &alpha);

		/**
		 * Set the layer used for a marker that is drawn in the text area, not the margin.
		 */
		void MarkerSetLayer(int32_t markerNumber, WinUIEditor::Layer const &layer);

		/**
		 * Set a margin to be either numeric or symbolic.
		 */
		void SetMarginTypeN(int32_t margin, WinUIEditor::MarginType const &marginType);

		/**
		 * Set the width of a margin to a width expressed in pixels.
		 */
		void SetMarginWidthN(int32_t margin, int32_t pixelWidth);

		/**
		 * Set a mask that determines which markers are displayed in a margin.
		 */
		void SetMarginMaskN(int32_t margin, int32_t mask);

		/**
		 * Make a margin sensitive or insensitive to mouse clicks.
		 */
		void SetMarginSensitiveN(int32_t margin, bool sensitive);

		/**
		 * Set the cursor shown when the mouse is inside a margin.
		 */
		void SetMarginCursorN(int32_t margin, WinUIEditor::CursorShape const &cursor);

		/**
		 * Set the background colour of a margin. Only visible for SC_MARGIN_COLOUR.
		 */
		void SetMarginBackN(int32_t margin, int32_t back);

		/**
		 * Set the foreground colour of a style.
		 */
		void StyleSetFore(int32_t style, int32_t fore);

		/**
		 * Set the background colour of a style.
		 */
		void StyleSetBack(int32_t style, int32_t back);

		/**
		 * Set a style to be bold or not.
		 */
		void StyleSetBold(int32_t style, bool bold);

		/**
		 * Set a style to be italic or not.
		 */
		void StyleSetItalic(int32_t style, bool italic);

		/**
		 * Set the size of characters of a style.
		 */
		void StyleSetSize(int32_t style, int32_t sizePoints);

		/**
		 * Set the font of a style.
		 */
		void StyleSetFontFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &fontName);
		void StyleSetFont(int32_t style, hstring const &fontName);

		/**
		 * Set a style to have its end of line filled or not.
		 */
		void StyleSetEOLFilled(int32_t style, bool eolFilled);

		/**
		 * Set a style to be underlined or not.
		 */
		void StyleSetUnderline(int32_t style, bool underline);

		/**
		 * Set a style to be mixed case, or to force upper or lower case.
		 */
		void StyleSetCase(int32_t style, WinUIEditor::CaseVisible const &caseVisible);

		/**
		 * Set the size of characters of a style. Size is in points multiplied by 100.
		 */
		void StyleSetSizeFractional(int32_t style, int32_t sizeHundredthPoints);

		/**
		 * Set the weight of characters of a style.
		 */
		void StyleSetWeight(int32_t style, WinUIEditor::FontWeight const &weight);

		/**
		 * Set the character set of the font in a style.
		 */
		void StyleSetCharacterSet(int32_t style, WinUIEditor::CharacterSet const &characterSet);

		/**
		 * Set a style to be a hotspot or not.
		 */
		void StyleSetHotSpot(int32_t style, bool hotspot);

		/**
		 * Indicate that a style may be monospaced over ASCII graphics characters which enables optimizations.
		 */
		void StyleSetCheckMonospaced(int32_t style, bool checkMonospaced);

		/**
		 * Set the stretch of characters of a style.
		 */
		void StyleSetStretch(int32_t style, WinUIEditor::FontStretch const &stretch);

		/**
		 * Set the invisible representation for a style.
		 */
		void StyleSetInvisibleRepresentationFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &representation);
		void StyleSetInvisibleRepresentation(int32_t style, hstring const &representation);

		/**
		 * Set the colour of an element. Translucency (alpha) may or may not be significant
		 * and this may depend on the platform. The alpha byte should commonly be 0xff for opaque.
		 */
		void SetElementColour(WinUIEditor::Element const &element, int32_t colourElement);

		/**
		 * Set a style to be visible or not.
		 */
		void StyleSetVisible(int32_t style, bool visible);

		/**
		 * Set the set of characters making up words for when moving or selecting by word.
		 * First sets defaults like SetCharsDefault.
		 */
		void SetWordCharsFromBuffer(Windows::Storage::Streams::IBuffer const &characters);
		void SetWordChars(hstring const &characters);

		/**
		 * Set an indicator to plain, squiggle or TT.
		 */
		void IndicSetStyle(int32_t indicator, WinUIEditor::IndicatorStyle const &indicatorStyle);

		/**
		 * Set the foreground colour of an indicator.
		 */
		void IndicSetFore(int32_t indicator, int32_t fore);

		/**
		 * Set an indicator to draw under text or over(default).
		 */
		void IndicSetUnder(int32_t indicator, bool under);

		/**
		 * Set a hover indicator to plain, squiggle or TT.
		 */
		void IndicSetHoverStyle(int32_t indicator, WinUIEditor::IndicatorStyle const &indicatorStyle);

		/**
		 * Set the foreground hover colour of an indicator.
		 */
		void IndicSetHoverFore(int32_t indicator, int32_t fore);

		/**
		 * Set the attributes of an indicator.
		 */
		void IndicSetFlags(int32_t indicator, WinUIEditor::IndicFlag const &flags);

		/**
		 * Set the stroke width of an indicator in hundredths of a pixel.
		 */
		void IndicSetStrokeWidth(int32_t indicator, int32_t hundredths);

		/**
		 * Used to hold extra styling information for each line.
		 */
		void SetLineState(int64_t line, int32_t state);

		/**
		 * Set a style to be changeable or not (read only).
		 * Experimental feature, currently buggy.
		 */
		void StyleSetChangeable(int32_t style, bool changeable);

		/**
		 * Define a set of characters that when typed will cause the autocompletion to
		 * choose the selected item.
		 */
		void AutoCSetFillUpsFromBuffer(Windows::Storage::Streams::IBuffer const &characterSet);
		void AutoCSetFillUps(hstring const &characterSet);

		/**
		 * Change the indentation of a line to a number of columns.
		 */
		void SetLineIndentation(int64_t line, int32_t indentation);

		/**
		 * Set selection from serialized form.
		 */
		void SetSelectionSerializedFromBuffer(Windows::Storage::Streams::IBuffer const &selectionString);
		void SetSelectionSerialized(hstring const &selectionString);

		/**
		 * Enlarge the number of lines allocated.
		 */
		void AllocateLines(int64_t lines);

		/**
		 * Set the start position in order to change when backspacing removes the calltip.
		 */
		void CallTipSetPosStart(int64_t posStart);

		/**
		 * Set the background colour for the call tip.
		 */
		void CallTipSetBack(int32_t back);

		/**
		 * Set the foreground colour for the call tip.
		 */
		void CallTipSetFore(int32_t fore);

		/**
		 * Set the foreground colour for the highlighted part of the call tip.
		 */
		void CallTipSetForeHlt(int32_t fore);

		/**
		 * Enable use of STYLE_CALLTIP and set call tip tab size in pixels.
		 */
		void CallTipUseStyle(int32_t tabSize);

		/**
		 * Set position of calltip, above or below text.
		 */
		void CallTipSetPosition(bool above);

		/**
		 * Set the fold level of a line.
		 * This encodes an integer level along with flags indicating whether the
		 * line is a header and whether it is effectively white space.
		 */
		void SetFoldLevel(int64_t line, WinUIEditor::FoldLevel const &level);

		/**
		 * Show the children of a header line.
		 */
		void SetFoldExpanded(int64_t line, bool expanded);

		/**
		 * Set some style options for folding.
		 */
		void SetFoldFlags(WinUIEditor::FoldFlag const &flags);

		/**
		 * Set a fore colour for active hotspots.
		 */
		void SetHotspotActiveFore(bool useSetting, int32_t fore);

		/**
		 * Set a back colour for active hotspots.
		 */
		void SetHotspotActiveBack(bool useSetting, int32_t back);

		/**
		 * Set the set of characters making up whitespace for when moving or selecting by word.
		 * Should be called after SetWordChars.
		 */
		void SetWhitespaceCharsFromBuffer(Windows::Storage::Streams::IBuffer const &characters);
		void SetWhitespaceChars(hstring const &characters);

		/**
		 * Set the set of characters making up punctuation characters
		 * Should be called after SetWordChars.
		 */
		void SetPunctuationCharsFromBuffer(Windows::Storage::Streams::IBuffer const &characters);
		void SetPunctuationChars(hstring const &characters);

		/**
		 * Set the string to separate parts when copying a multiple selection.
		 */
		void SetCopySeparatorFromBuffer(Windows::Storage::Streams::IBuffer const &separator);
		void SetCopySeparator(hstring const &separator);

		/**
		 * Set the alpha fill colour of the given indicator.
		 */
		void IndicSetAlpha(int32_t indicator, WinUIEditor::Alpha const &alpha);

		/**
		 * Set the alpha outline colour of the given indicator.
		 */
		void IndicSetOutlineAlpha(int32_t indicator, WinUIEditor::Alpha const &alpha);

		/**
		 * Set the text in the text margin for a line
		 */
		void MarginSetTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		void MarginSetText(int64_t line, hstring const &text);

		/**
		 * Set the style number for the text margin for a line
		 */
		void MarginSetStyle(int64_t line, int32_t style);

		/**
		 * Set the style in the text margin for a line
		 */
		void MarginSetStylesFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles);
		void MarginSetStyles(int64_t line, hstring const &styles);

		/**
		 * Set the annotation text for a line
		 */
		void AnnotationSetTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		void AnnotationSetText(int64_t line, hstring const &text);

		/**
		 * Set the style number for the annotations for a line
		 */
		void AnnotationSetStyle(int64_t line, int32_t style);

		/**
		 * Set the annotation styles for a line
		 */
		void AnnotationSetStylesFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &styles);
		void AnnotationSetStyles(int64_t line, hstring const &styles);

		/**
		 * Set the caret position of the nth selection.
		 */
		void SetSelectionNCaret(int32_t selection, int64_t caret);

		/**
		 * Set the anchor position of the nth selection.
		 */
		void SetSelectionNAnchor(int32_t selection, int64_t anchor);

		/**
		 * Set the virtual space of the caret of the nth selection.
		 */
		void SetSelectionNCaretVirtualSpace(int32_t selection, int64_t space);

		/**
		 * Set the virtual space of the anchor of the nth selection.
		 */
		void SetSelectionNAnchorVirtualSpace(int32_t selection, int64_t space);

		/**
		 * Sets the position that starts the selection - this becomes the anchor.
		 */
		void SetSelectionNStart(int32_t selection, int64_t anchor);

		/**
		 * Sets the position that ends the selection - this becomes the currentPosition.
		 */
		void SetSelectionNEnd(int32_t selection, int64_t caret);

		/**
		 * Set the foreground colour of additional selections.
		 * Must have previously called SetSelFore with non-zero first argument for this to have an effect.
		 */
		void SetAdditionalSelFore(int32_t fore);

		/**
		 * Set the background colour of additional selections.
		 * Must have previously called SetSelBack with non-zero first argument for this to have an effect.
		 */
		void SetAdditionalSelBack(int32_t back);

		/**
		 * Set the width for future RGBA image data.
		 */
		void RGBAImageSetWidth(int32_t width);

		/**
		 * Set the height for future RGBA image data.
		 */
		void RGBAImageSetHeight(int32_t height);

		/**
		 * Set the scale factor in percent for future RGBA image data.
		 */
		void RGBAImageSetScale(int32_t scalePercent);

		/**
		 * Set the way a character is drawn.
		 */
		void SetRepresentationFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, Windows::Storage::Streams::IBuffer const &representation);
		void SetRepresentation(hstring const &encodedCharacter, hstring const &representation);

		/**
		 * Set the appearance of a representation.
		 */
		void SetRepresentationAppearanceFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, WinUIEditor::RepresentationAppearance const &appearance);
		void SetRepresentationAppearance(hstring const &encodedCharacter, WinUIEditor::RepresentationAppearance const &appearance);

		/**
		 * Set the colour of a representation.
		 */
		void SetRepresentationColourFromBuffer(Windows::Storage::Streams::IBuffer const &encodedCharacter, int32_t colour);
		void SetRepresentationColour(hstring const &encodedCharacter, int32_t colour);

		/**
		 * Set the end of line annotation text for a line
		 */
		void EOLAnnotationSetTextFromBuffer(int64_t line, Windows::Storage::Streams::IBuffer const &text);
		void EOLAnnotationSetText(int64_t line, hstring const &text);

		/**
		 * Set the style number for the end of line annotations for a line
		 */
		void EOLAnnotationSetStyle(int64_t line, int32_t style);

		/**
		 * Set up a value that may be used by a lexer for some optional feature.
		 */
		void SetPropertyFromBuffer(Windows::Storage::Streams::IBuffer const &key, Windows::Storage::Streams::IBuffer const &value);
		void SetProperty(hstring const &key, hstring const &value);

		/**
		 * Set up the key words used by the lexer.
		 */
		void SetKeyWordsFromBuffer(int32_t keyWordSet, Windows::Storage::Streams::IBuffer const &keyWords);
		void SetKeyWords(int32_t keyWordSet, hstring const &keyWords);

		/**
		 * Set the identifiers that are shown in a particular style
		 */
		void SetIdentifiersFromBuffer(int32_t style, Windows::Storage::Streams::IBuffer const &identifiers);
		void SetIdentifiers(int32_t style, hstring const &identifiers);

		/**
		 * Set the lexer from an ILexer*.
		 */
		void SetILexer(uint64_t ilexer);

	private:
#include "EditorExtensionsState.h"

		event<WinUIEditor::StyleNeededHandler> _styleNeededEvent;
		event<WinUIEditor::CharAddedHandler> _charAddedEvent;
		event<WinUIEditor::SavePointReachedHandler> _savePointReachedEvent;
		event<WinUIEditor::SavePointLeftHandler> _savePointLeftEvent;
		event<WinUIEditor::ModifyAttemptROHandler> _modifyAttemptROEvent;
		event<WinUIEditor::KeyHandler> _keyEvent;
		event<WinUIEditor::DoubleClickHandler> _doubleClickEvent;
		event<WinUIEditor::UpdateUIHandler> _updateUIEvent;
		event<WinUIEditor::ModifiedHandler> _modifiedEvent;
		event<WinUIEditor::MacroRecordHandler> _macroRecordEvent;
		event<WinUIEditor::MarginClickHandler> _marginClickEvent;
		event<WinUIEditor::NeedShownHandler> _needShownEvent;
		event<WinUIEditor::PaintedHandler> _paintedEvent;
		event<WinUIEditor::UserListSelectionHandler> _userListSelectionEvent;
		event<WinUIEditor::URIDroppedHandler> _uRIDroppedEvent;
		event<WinUIEditor::DwellStartHandler> _dwellStartEvent;
		event<WinUIEditor::DwellEndHandler> _dwellEndEvent;
		event<WinUIEditor::ZoomChangedHandler> _zoomChangedEvent;
		event<WinUIEditor::HotSpotClickHandler> _hotSpotClickEvent;
		event<WinUIEditor::HotSpotDoubleClickHandler> _hotSpotDoubleClickEvent;
		event<WinUIEditor::CallTipClickHandler> _callTipClickEvent;
		event<WinUIEditor::AutoCSelectionHandler> _autoCSelectionEvent;
		event<WinUIEditor::IndicatorClickHandler> _indicatorClickEvent;
		event<WinUIEditor::IndicatorReleaseHandler> _indicatorReleaseEvent;
		event<WinUIEditor::AutoCCancelledHandler> _autoCCancelledEvent;
		event<WinUIEditor::AutoCCharDeletedHandler> _autoCCharDeletedEvent;
		event<WinUIEditor::HotSpotReleaseClickHandler> _hotSpotReleaseClickEvent;
		event<WinUIEditor::FocusInHandler> _focusInEvent;
		event<WinUIEditor::FocusOutHandler> _focusOutEvent;
		event<WinUIEditor::AutoCCompletedHandler> _autoCCompletedEvent;
		event<WinUIEditor::MarginRightClickHandler> _marginRightClickEvent;
		event<WinUIEditor::AutoCSelectionChangeHandler> _autoCSelectionChangeEvent;

		weak_ref<EditorBaseControl> _editor{ nullptr };
	};
}
