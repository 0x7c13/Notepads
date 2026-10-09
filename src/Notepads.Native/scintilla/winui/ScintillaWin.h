// Scintilla source code edit control
/** @file ScintillaWin.h
 ** Define functions from ScintillaWin.cxx that can be called from ScintillaDLL.cxx.
 **/
 // Copyright 1998-2018 by Neil Hodgson <neilh@scintilla.org>
 // The License.txt file describes the conditions under which this software may be distributed.

// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once

#include "Wrapper.h"
#include "MainWrapper.h"
#include "CallTipWrapper.h"
#include "DisplayLineMap.h"
#include <winrt/WinUIEditor.h>

namespace WinUIEditor { class NativeDocumentJournal; class NativeJournalFile; struct JournalPrefix; }
namespace WinUIEditor { class SyntaxHighlightingState; }

namespace Scintilla::Internal {
	enum class ScrollEventType : int32_t
	{
		SmallDecrement = 0,
		SmallIncrement = 1,
		LargeDecrement = 2,
		LargeIncrement = 3,
		ThumbPosition = 4,
		ThumbTrack = 5,
		First = 6,
		Last = 7,
		EndScroll = 8,
	};

	class MouseWheelDelta {
		int wheelDelta = 0;
	public:
		bool Accumulate(int delta) noexcept {
			wheelDelta -= delta;
			return std::abs(wheelDelta) >= WHEEL_DELTA;
		}
		int Actions() noexcept {
			const int actions = wheelDelta / WHEEL_DELTA;
			wheelDelta = wheelDelta % WHEEL_DELTA;
			return actions;
		}
	};

	struct HorizontalScrollRange {
		int pageWidth;
		int documentWidth;
	};

	// TSF message queue
	class IMessage
	{
	public:
		IMessage() {}
		virtual ~IMessage() {}
		virtual uint8_t Type() const = 0;
	};

	class CallTipCallback;

	class ScintillaWinUI :
		public ScintillaBase,
		public ::winrt::implements<ScintillaWinUI, ::IVirtualSurfaceUpdatesCallbackNative, ITextStoreACP2, ITfContextOwnerCompositionSink>
	{
		friend class CallTipCallback;

	public:
		ScintillaWinUI(std::shared_ptr<WinUIEditor::MainWrapper> const &wrapper);
		// Deleted so ScintillaWinUI objects can not be copied.
		ScintillaWinUI(const ScintillaWinUI &) = delete;
		ScintillaWinUI(ScintillaWinUI &&) = delete;
		ScintillaWinUI &operator=(const ScintillaWinUI &) = delete;
		ScintillaWinUI &operator=(ScintillaWinUI &&) = delete;
		void DpiChanged();
		void SizeChanged();
		void FocusChanged(bool focused);
		void PointerPressed(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void RightPointerPressed(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void PointerMoved(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers, winrt::DUI::PointerPoint const &pointerPoint);
		void PointerReleased(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void PointerWheelChanged(int delta, bool horizontal, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void PointerExited();
		void HorizontalScroll(ScrollEventType event, int value);
		void Scroll(ScrollEventType event, int value);
		void KeyDown(winrt::Windows::System::VirtualKey key, winrt::Windows::System::VirtualKeyModifiers modifiers, bool const isExtendedKey, bool *handled);
		void StopTimers();
		void ResumeIdleWork();
		void SetSyntaxLexer(winrt::hstring const &name, winrt::array_view<winrt::hstring const> keywords,
			winrt::array_view<winrt::hstring const> propertyNames, winrt::array_view<winrt::hstring const> propertyValues);
		winrt::WinUIEditor::EditorSyntaxPauseReason SyntaxHighlightingPauseReason() const noexcept;
		void Finalize();
		void FinalizeAfterApartmentClosed() noexcept;
		void AcquireReadLease();
		void ReleaseReadLease() noexcept;
		void ReplaceDocumentWithUndo(Document &replacement);
		void PublishPreparedReplacement(Document &replacement, const char *previousText, std::unique_ptr<IContractionState> contraction, bool leased = false);
		const Document &LeasedDocument() const;
		void SetDisplayLineMap(std::vector<DisplayGap> gaps, std::vector<TintedLineRange> tints);
		void SetDisplayColours(unsigned int line, unsigned int gap, unsigned int hatch);
		void SetDisplayHighlight(int64_t row, int64_t count, unsigned int colour);
		std::shared_ptr<WinUIEditor::NativeDocumentJournal> PrepareJournalReplacement();
		uint64_t DocumentSequence() const noexcept;
		uint64_t DocumentRevision() const noexcept { return _documentRevision; }
		uint64_t SelectionRevision() const noexcept { return selectionRevision; }
		bool HasJournal() const noexcept;
		bool HasTextStoreLock() const noexcept { return _lock != NONE; }
		bool IsComposing() const noexcept { return _compositionCount != 0; }
		void StartJournal(std::shared_ptr<WinUIEditor::NativeJournalFile> file, uint64_t sequence, bool rotate);
		WinUIEditor::JournalPrefix AcquireJournalPrefix();
		std::shared_ptr<WinUIEditor::NativeJournalFile> ActiveJournalFile() const noexcept;
		void StopJournals() noexcept;
		std::vector<std::shared_ptr<WinUIEditor::NativeJournalFile>> StopJournal();
		~ScintillaWinUI();
		void CharacterReceived(char16_t character);
		bool ShouldShowContextMenu(winrt::Windows::Foundation::Point const &point);
		void HandleCallTipClick(winrt::Windows::Foundation::Point const &point);
		void HandleCallTipPointerMove(winrt::Windows::Foundation::Point const &point);

		sptr_t GetTextLength();
		sptr_t GetText(uptr_t wParam, sptr_t lParam);
		bool SetText(std::wstring_view const &text);
		void PasteText(std::wstring_view text);

		void StyleSetForeTransparent(int style, ColourRGBA color);
		void StyleSetBackTransparent(int style, ColourRGBA color);
		void SetCallTipHoverColor(ColourRGBA color);
		void SetCallTipBackgroundColorTransparent(ColourRGBA color);
		void PublicInvalidateStyleRedraw();
		void StyleClearCustom();
		void SetFoldMarginColorTransparent(bool useSetting, ColourRGBA back);
		void SetFoldMarginHiColorTransparent(bool useSetting, ColourRGBA fore);

		void SetWndProc(std::function<LRESULT(winrt::Windows::Foundation::IInspectable const &, UINT, WPARAM, LPARAM)> wndProc);
		void SetWndProcTag(winrt::Windows::Foundation::IInspectable const &tag);

		void DragEnter(winrt::Windows::ApplicationModel::DataTransfer::DataPackageView const &dataView, winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation const &allowedOperations, winrt::Windows::ApplicationModel::DataTransfer::DragDrop::DragDropModifiers const &modifiers, winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation &operation);
		void DragOver(winrt::Windows::Foundation::Point const &point, winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation const &allowedOperations, winrt::Windows::ApplicationModel::DataTransfer::DragDrop::DragDropModifiers const &modifiers, winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation &operation);
		void DragLeave();
		winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation> DropAsync(winrt::Windows::Foundation::Point point, winrt::Windows::ApplicationModel::DataTransfer::DataPackageView dataView, winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation allowedOperations, winrt::Windows::ApplicationModel::DataTransfer::DragDrop::DragDropModifiers modifiers);
		std::string_view GetDragData();
		static sptr_t DirectFunction(sptr_t ptr, UINT iMessage, uptr_t wParam, sptr_t lParam);
		static sptr_t DirectStatusFunction(sptr_t ptr, UINT iMessage, uptr_t wParam, sptr_t lParam, int *pStatus);
		sptr_t WndProc(Message iMessage, uptr_t wParam, sptr_t lParam) override;

	private:
		unsigned int _readLeases{ 0 };
		std::shared_ptr<WinUIEditor::NativeDocumentJournal> _journal;
		std::vector<std::shared_ptr<WinUIEditor::NativeJournalFile>> _retiringJournals;
		uint64_t _documentSequence{};
		uint64_t _documentRevision{};
		bool _readOnlyBeforeLease{ false };
		void UpdateHorizontalScrollBarInset();
		bool _tsfCore;

		bool hasOKText{ false };

		std::function<LRESULT(winrt::Windows::Foundation::IInspectable const &, UINT, WPARAM, LPARAM)> _wndProc;
		winrt::weak_ref<winrt::Windows::Foundation::IInspectable> _wndProcTag;
		LRESULT SendMessage(UINT msg, WPARAM wParam, LPARAM lParam);

		Sci::Position AcpToDocPosition(Sci::Position acp, bool *acpWithinSurrogatePair = nullptr);
		Sci::Position DocPositionToAcp(Sci::Position docPosition);

		void ProcessMessage(std::unique_ptr<IMessage> const &message);
		void ProcessKeyDownMessage(winrt::Windows::System::VirtualKey key, winrt::Windows::System::VirtualKeyModifiers modifiers, bool const isExtendedKey, bool *handled);
		void ProcessPointerPressedMessage(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void ProcessRightPointerPressedMessage(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void ProcessPointerMovedMessage(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers, winrt::DUI::PointerPoint const &pointerPoint);
		void ProcessPointerReleasedMessage(winrt::Windows::Foundation::Point const &point, uint64_t timestamp, winrt::Windows::System::VirtualKeyModifiers modifiers);
		void ProcessNotifyMessage(uptr_t wParam, NotificationData const &notificationData, bool notifyTsf, Sci::Position deletedUtf16Length);
		void NotifyModified(Document *document, DocModification modification, void *userData) override;
		Sci::Position _deletedUtf16Length{ 0 };
		void ProcessCharacterReceivedMessage(char16_t character);
		winrt::DUI::PointerPoint _dragPointer{ nullptr };
		uint64_t _dragDocumentRevision{};
		std::string _dragSelection;

		Scintilla::KeyMod WindowsModifiers(winrt::Windows::System::VirtualKeyModifiers modifiers);
		void AddWString(std::wstring_view wsv, CharacterSource charSource); // win32
		std::string EncodeWString(std::wstring_view wsv);
		wchar_t lastHighSurrogateChar{ 0 }; // win32
		bool lastKeyDownConsumed{ false }; // win32?
		void ImeEndComposition();
		std::queue<std::unique_ptr<IMessage>> msgq{}; //31
		std::queue<std::unique_ptr<IMessage>> notifyq{}; //3000
		void ProcessQueues();
		bool _shouldNotifyTsf{ true };
		bool _fromNotifyQueue{ false };
		Sci::Position _multiInsert{ 0 };
		Sci::Position _multiCorrect{ 0 };

		// WinUI Todo: These two values should be updated to use the Windows setting
		int _lastVerticalScrollDelta{ 0 };
		int _lastHorizontalScrollDelta{ 0 };
		MouseWheelDelta verticalWheelDelta{};
		MouseWheelDelta horizontalWheelDelta{};

		winrt::Windows::UI::Text::Core::CoreTextEditContext _editContext{ nullptr };
		winrt::Windows::UI::Text::Core::CoreTextEditContext::TextRequested_revoker _textRequestedRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::SelectionRequested_revoker _selectionRequestedRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::FocusRemoved_revoker _focusRemovedRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::TextUpdating_revoker _textUpdatingRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::SelectionUpdating_revoker _selectionUpdatingRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::FormatUpdating_revoker _formatUpdatingRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::LayoutRequested_revoker _layoutRequestedRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::CompositionStarted_revoker _compositionStartedRevoker{};
		winrt::Windows::UI::Text::Core::CoreTextEditContext::CompositionCompleted_revoker _compositionCompletedRevoker{};
		void OnTextRequested(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextTextRequestedEventArgs const &args);
		void OnSelectionRequested(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextSelectionRequestedEventArgs const &args);
		void OnFocusRemoved(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::Foundation::IInspectable const &args);
		void OnTextUpdating(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextTextUpdatingEventArgs const &args);
		void OnSelectionUpdating(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextSelectionUpdatingEventArgs const &args);
		void OnFormatUpdating(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextFormatUpdatingEventArgs const &args);
		void OnLayoutRequested(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextLayoutRequestedEventArgs const &args);
		void OnCompositionStarted(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextCompositionStartedEventArgs const &args);
		void OnCompositionCompleted(winrt::Windows::UI::Text::Core::CoreTextEditContext const &sender, winrt::Windows::UI::Text::Core::CoreTextCompositionCompletedEventArgs const &args);
		bool TsfCoreLock(DWORD lock);
		void TsfCoreUnlock();

		winrt::com_ptr<ITfThreadMgr2> _tfThreadManager{ nullptr };
		winrt::com_ptr<ITfDocumentMgr> _tfDocumentManager{ nullptr };
		winrt::com_ptr<ITfContext> _tfContext{ nullptr };
		winrt::com_ptr<IUnknown> _sinkUnk{ nullptr };
		winrt::com_ptr<ITextStoreACPSink> _tfTextStoreACPSink{ nullptr };
		DWORD  _textStoreSinkMask{ 0 };
		TfEditCookie _tfEditCookie{ 0 };
		TfClientId _tfClientId{ 0 };
		unsigned int _compositionCount{ 0 };
		enum LockTypes
		{
			NONE = 0,
			READONLY = TS_LF_READ,
			READWRITE = TS_LF_READWRITE
		} _lock{ NONE }, _lockAsync{ NONE };
		IFACEMETHOD(AdviseSink)(const IID &, IUnknown *, DWORD) override;
		IFACEMETHOD(UnadviseSink)(IUnknown *) override;
		IFACEMETHOD(RequestLock)(DWORD, HRESULT *) override;
		IFACEMETHOD(GetStatus)(TS_STATUS *) override;
		IFACEMETHOD(QueryInsert)(LONG, LONG, ULONG, LONG *, LONG *) override;
		IFACEMETHOD(GetSelection)(ULONG, ULONG, TS_SELECTION_ACP *, ULONG *) override;
		IFACEMETHOD(SetSelection)(ULONG, const TS_SELECTION_ACP *) override;
		IFACEMETHOD(GetText)(LONG, LONG, WCHAR *, ULONG, ULONG *, TS_RUNINFO *, ULONG, ULONG *, LONG *) override;
		IFACEMETHOD(SetText)(DWORD, LONG, LONG, const WCHAR *, ULONG, TS_TEXTCHANGE *) override;
		IFACEMETHOD(GetFormattedText)(LONG, LONG, IDataObject **) override;
		IFACEMETHOD(GetEmbedded)(LONG, const GUID &, const IID &, IUnknown **) override;
		IFACEMETHOD(QueryInsertEmbedded)(const GUID *, const FORMATETC *, BOOL *) override;
		IFACEMETHOD(InsertEmbedded)(DWORD, LONG, LONG, IDataObject *, TS_TEXTCHANGE *) override;
		IFACEMETHOD(InsertTextAtSelection)(DWORD, const WCHAR *, ULONG, LONG *, LONG *, TS_TEXTCHANGE *) override;
		IFACEMETHOD(InsertEmbeddedAtSelection)(DWORD, IDataObject *, LONG *, LONG *, TS_TEXTCHANGE *) override;
		IFACEMETHOD(RequestSupportedAttrs)(DWORD, ULONG, const TS_ATTRID *) override;
		IFACEMETHOD(RequestAttrsAtPosition)(LONG, ULONG, const TS_ATTRID *, DWORD) override;
		IFACEMETHOD(RequestAttrsTransitioningAtPosition)(LONG, ULONG, const TS_ATTRID *, DWORD) override;
		IFACEMETHOD(FindNextAttrTransition)(LONG, LONG, ULONG, const TS_ATTRID *, DWORD, LONG *, BOOL *, LONG *) override;
		IFACEMETHOD(RetrieveRequestedAttrs)(ULONG, TS_ATTRVAL *, ULONG *) override;
		IFACEMETHOD(GetEndACP)(LONG *) override;
		IFACEMETHOD(GetActiveView)(TsViewCookie *) override;
		IFACEMETHOD(GetACPFromPoint)(TsViewCookie, const POINT *, DWORD, LONG *) override;
		IFACEMETHOD(GetTextExt)(TsViewCookie, LONG, LONG, RECT *, BOOL *) override;
		IFACEMETHOD(GetScreenExt)(TsViewCookie, RECT *) override;

		IFACEMETHOD(OnEndComposition)(ITfCompositionView *pComposition);
		IFACEMETHOD(OnStartComposition)(ITfCompositionView *pComposition, BOOL *pfOk);
		IFACEMETHOD(OnUpdateComposition)(ITfCompositionView *pComposition, ITfRange *pRangeNew);

		std::shared_ptr<::WinUIEditor::MainWrapper> _mainWrapper{ nullptr };

		// Timer implementation
		// Todo: Planning to not use XAML APIs on the Scintilla side, so replace DispatcherTimer with something that works outside of XAML
		// Candidates include DispatcherQueueTimer (not available on 1703) or ThreadPoolTimer, which should work in UWP and WinUI 3
		// TickReason: caret, scroll, widen, dwell, platform (unused?)
		winrt::DUX::DispatcherTimer _caretTimer{};
		winrt::DUX::DispatcherTimer _scrollTimer{};
		winrt::DUX::DispatcherTimer _widenTimer{};
		winrt::DUX::DispatcherTimer _dwellTimer{};
		winrt::DUX::DispatcherTimer _idleTimer{};
		winrt::DUX::DispatcherTimer::Tick_revoker _idleTickRevoker{};
		bool _idleSuspended{};
		bool _idleFinalized{};
		std::shared_ptr<WinUIEditor::SyntaxHighlightingState> _syntaxState;
		bool _syntaxMemoryPaused{};
		void PauseSyntaxForMemory();
		void ResetSyntaxStyles();
		void NotifyStylingCompleted(Document *document, void *userData) override;
		bool SetIdle(bool enabled) override;
		void QueueIdleWork(WorkItems items, Sci::Position upTo = 0) override;
		void IdleWork() override;
		void OnIdleTimerTick(winrt::Windows::Foundation::IInspectable const &sender, winrt::Windows::Foundation::IInspectable const &args);
		void OnCaretTimerTick(winrt::Windows::Foundation::IInspectable const &sender, winrt::Windows::Foundation::IInspectable const &args);
		void OnScrollTimerTick(winrt::Windows::Foundation::IInspectable const &sender, winrt::Windows::Foundation::IInspectable const &args);
		void OnWidenTimerTick(winrt::Windows::Foundation::IInspectable const &sender, winrt::Windows::Foundation::IInspectable const &args);
		void OnDwellTimerTick(winrt::Windows::Foundation::IInspectable const &sender, winrt::Windows::Foundation::IInspectable const &args);
		winrt::DUX::DispatcherTimer::Tick_revoker _caretTickRevoker{};
		winrt::DUX::DispatcherTimer::Tick_revoker _scrollTickRevoker{};
		winrt::DUX::DispatcherTimer::Tick_revoker _widenTickRevoker{};
		winrt::DUX::DispatcherTimer::Tick_revoker _dwellTickRevoker{};
		winrt::DUX::DispatcherTimer GetTimerForReason(TickReason reason);

		virtual bool FineTickerRunning(TickReason reason) override;
		virtual void FineTickerStart(TickReason reason, int millis, int tolerance) override;
		virtual void FineTickerCancel(TickReason reason) override;
		void ScrollText(Sci::Line linesToMove) override;
		virtual void SetVerticalScrollPos() override;
		virtual void SetHorizontalScrollPos() override;
		void HorizontalScrollToClamped(int xPos);
		[[nodiscard]] HorizontalScrollRange GetHorizontalScrollRange() const;
		virtual bool ModifyScrollBars(Sci::Line nMax, Sci::Line nPage) override;
		bool ChangeScrollRange(int nBar, int nMin, int nMax, UINT nPage) noexcept;
		void ChangeScrollPos(int barType, Sci::Position pos);
		virtual std::unique_ptr<CaseFolder> CaseFolderForEncoding() override;
		virtual std::string CaseMapString(const std::string &s, CaseMapping caseMapping) override;
		virtual void Copy() override;
		virtual void Paste() override;
		winrt::fire_and_forget PasteAsync();
		virtual void ClaimSelection() override;
		virtual void NotifyChange() override;
		virtual void NotifyFocus(bool focus) override;
		virtual void NotifyParent(Scintilla::NotificationData scn) override;
		virtual void CopyToClipboard(const SelectionText &selectedText) override;
		virtual void SetMouseCapture(bool on) override;
		virtual bool HaveMouseCapture() override;
		[[nodiscard]] virtual bool ValidCodePage(int codePage) const override;
		[[nodiscard]] virtual std::string UTF8FromEncoded(std::string_view encoded) const override;
		[[nodiscard]] virtual std::string EncodedFromUTF8(std::string_view utf8) const override;
		virtual Scintilla::sptr_t DefWndProc(Scintilla::Message iMessage, Scintilla::uptr_t wParam, Scintilla::sptr_t lParam) override;
		virtual void CreateCallTipWindow(PRectangle rc) override;
		virtual void AddToPopUp(const char *label, int cmd = 0, bool enabled = true) override;
		IFACEMETHOD(UpdatesNeeded)() override;
		void DrawBit(RECT const &drawingBounds);
		[[nodiscard]] UINT CodePageOfDocument() const noexcept;
		winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation EffectFromState(winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation const &allowedOperations, winrt::Windows::ApplicationModel::DataTransfer::DragDrop::DragDropModifiers const &grfKeyState) const noexcept;
		void StartDrag() override;
		winrt::fire_and_forget DoDragAsync();
		sptr_t OnSetDocPointer(uptr_t wParam, sptr_t lParam);
	};

	class CallTipCallback : public ::winrt::implements<CallTipCallback, ::IVirtualSurfaceUpdatesCallbackNative>
	{
	public:
		CallTipCallback(std::shared_ptr<WinUIEditor::Wrapper> const &wrapper, winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> const &scintilla);
	private:
		winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> _scintilla;
		IFACEMETHOD(UpdatesNeeded)() override;
		void DrawBit(RECT const &drawingBounds);
		std::shared_ptr<::WinUIEditor::Wrapper> _wrapper{ nullptr };
	};
}
