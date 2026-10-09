// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once

#include "EditorBaseControl.g.h"

#include "ScintillaWin.h"
#include "ScintillaCall.h"
#include "Wrapper.h"
#include "MainWrapper.h"
#include "DisplayLineMap.h"
#include <atomic>

namespace winrt::WinUIEditor::implementation
{
	struct EditorBaseControl : EditorBaseControlT<EditorBaseControl>
	{
		EditorBaseControl();
		~EditorBaseControl();
		static winrt::fire_and_forget final_release(std::unique_ptr<EditorBaseControl> self) noexcept;

		WinUIEditor::Editor Editor();
		bool IsComposing() const noexcept { return _scintilla->IsComposing(); }
		bool HasTextStoreLock() const noexcept { return _scintilla->HasTextStoreLock(); }
		bool MouseWheelZoomEnabled() const noexcept { return _mouseWheelZoomEnabled; }
		void MouseWheelZoomEnabled(bool value) noexcept { _mouseWheelZoomEnabled = value; }
		void SetBackgroundColor(Windows::UI::Color const &color);
		void PasteText(hstring const &text);
		void SetSyntaxLexer(hstring const &name, array_view<hstring const> keywords,
			array_view<hstring const> propertyNames, array_view<hstring const> propertyValues)
		{ _scintilla->SetSyntaxLexer(name, keywords, propertyNames, propertyValues); }
		WinUIEditor::EditorSyntaxPauseReason SyntaxHighlightingPauseReason() const noexcept { return _scintilla->SyntaxHighlightingPauseReason(); }
		void FinalizeAfterApartmentClosed() noexcept;
		void AcquireReadLease();
		void ReleaseReadLease() noexcept;
		void ReplaceDocumentWithUndo(Scintilla::Internal::Document &replacement);
		void SetLoadedDocument(Scintilla::Internal::Document &replacement);
		void SetDisplayLineMap(std::vector<Scintilla::Internal::DisplayGap> gaps, std::vector<Scintilla::Internal::TintedLineRange> tints);
		void SetDisplayColours(unsigned int line, unsigned int gap, unsigned int hatch);
		void SetDisplayHighlight(int64_t row, int64_t count, unsigned int colour);
		void PublishPreparedReplacement(Scintilla::Internal::Document &replacement, const char *previousText,
			std::unique_ptr<Scintilla::Internal::IContractionState> contraction, bool leased = false);
		const Scintilla::Internal::Document &LeasedDocument() const;
		std::shared_ptr<::WinUIEditor::NativeDocumentJournal> PrepareJournalReplacement();
		bool IsFinalized() const noexcept { return _uiResourcesFinalized.load(); }
		uint64_t DocumentSequence() const noexcept;
		uint64_t DocumentRevision() const noexcept;
		uint64_t SelectionRevision() const noexcept;
		bool HasJournal() const noexcept;
		Windows::Foundation::IAsyncAction WaitForTextStoreUnlockAsync();
		void StartJournal(std::shared_ptr<::WinUIEditor::NativeJournalFile> file, uint64_t sequence, bool rotate);
		::WinUIEditor::JournalPrefix AcquireJournalPrefix();
		std::shared_ptr<::WinUIEditor::NativeJournalFile> ActiveJournalFile() const noexcept;
		std::vector<std::shared_ptr<::WinUIEditor::NativeJournalFile>> StopJournal();
		std::shared_ptr<Scintilla::ScintillaCall> Call() noexcept { return _call; }

		void OnApplyTemplate();
		void OnGotFocus(DUX::RoutedEventArgs const &e);
		void OnLostFocus(DUX::RoutedEventArgs const &e);
		void OnKeyDown(DUX::Input::KeyRoutedEventArgs const &e);
		void OnKeyUp(DUX::Input::KeyRoutedEventArgs const &e);
		void OnPointerPressed(DUX::Input::PointerRoutedEventArgs const &e);
		void OnPointerReleased(DUX::Input::PointerRoutedEventArgs const &e);
		DUX::Automation::Peers::AutomationPeer OnCreateAutomationPeer();

		Scintilla::sptr_t PublicWndProc(Scintilla::Message iMessage, Scintilla::uptr_t wParam, Scintilla::sptr_t lParam);
		int64_t SendMessage(ScintillaMessage const &message, uint64_t wParam, int64_t lParam);

		void StyleSetForeTransparent(int style, Scintilla::Internal::ColourRGBA color);
		void StyleSetBackTransparent(int style, Scintilla::Internal::ColourRGBA color);
		void InvalidateStyleRedraw();
		void StyleClearCustom();
		void SetFoldMarginColorTransparent(bool useSetting, Scintilla::Internal::ColourRGBA back);
		void SetFoldMarginHiColorTransparent(bool useSetting, Scintilla::Internal::ColourRGBA fore);
		void SetCallTipBackgroundColorTransparent(Scintilla::Internal::ColourRGBA color);
		void SetCallTipHoverColor(Scintilla::Internal::ColourRGBA color);

		event_token DpiChanged(Windows::Foundation::EventHandler<double> const &handler);
		void DpiChanged(event_token const &token) noexcept;

		event_token NotifyMessageReceived(Windows::Foundation::EventHandler<int64_t> const &handler);
		void NotifyMessageReceived(event_token const &token) noexcept;
		float DpiScale();

		event<Windows::Foundation::EventHandler<int64_t>> InternalNotifyMessageReceived;

	private:
		winrt::apartment_context _owningApartment;
		DWORD _owningThreadId{ ::GetCurrentThreadId() };
		std::atomic<bool> _uiResourcesFinalized{ false };
		DUD::DispatcherQueue::ShutdownStarting_revoker _shutdownStartingRevoker;
		Windows::UI::Core::CoreWindow::Closed_revoker _coreWindowClosedRevoker;
		void FinalizeUIResources();
		bool _hasFcu{ Windows::Foundation::Metadata::ApiInformation::IsApiContractPresent(L"Windows.Foundation.UniversalApiContract", 5) }; // Todo: Make static
		bool _hasXamlRoot{ Windows::Foundation::Metadata::ApiInformation::IsTypePresent(L"Windows.UI.Xaml.XamlRoot") }; // Todo: Make static
		bool _hasIsLoaded{ Windows::Foundation::Metadata::ApiInformation::IsPropertyPresent(L"Windows.UI.Xaml.FrameworkElement", L"IsLoaded") }; // Todo: Make static
		bool _isPointerOver{ false };
		Windows::Graphics::Display::DisplayInformation _displayInformation{ nullptr };
		bool _isLoaded{ false };
		bool _isFocused{ false };
		bool _mouseWheelZoomEnabled{ true };
		WinUIEditor::Editor _editorWrapper{ nullptr };
		std::shared_ptr<Scintilla::ScintillaCall> _call{ nullptr };
		com_ptr<::Scintilla::Internal::ScintillaWinUI> _scintilla{ nullptr };
		float _dpiScale{ 0 };
		event<Windows::Foundation::EventHandler<double>> _dpiChangedEvent;
		event<Windows::Foundation::EventHandler<int64_t>> _notifyMessageReceived;
		DUX::FrameworkElement::SizeChanged_revoker _imageTargetSizeChangedRevoker{};
		void ImageTarget_SizeChanged(Windows::Foundation::IInspectable const &sender, DUX::SizeChangedEventArgs const &args);
		DUX::UIElement::PointerMoved_revoker _imageTargetPointerMovedRevoker{};
		void ImageTarget_PointerMoved(Windows::Foundation::IInspectable const &sender, DUX::Input::PointerRoutedEventArgs const &e);
		DUX::UIElement::PointerCaptureLost_revoker _imageTargetPointerCaptureLostRevoker{};
		void ImageTarget_PointerCaptureLost(Windows::Foundation::IInspectable const &sender, DUX::Input::PointerRoutedEventArgs const &e);
		DUX::UIElement::PointerEntered_revoker _imageTargetPointerEnteredRevoker{};
		void ImageTarget_PointerEntered(Windows::Foundation::IInspectable const &sender, DUX::Input::PointerRoutedEventArgs const &e);
		DUX::UIElement::PointerExited_revoker _imageTargetPointerExitedRevoker{};
		void ImageTarget_PointerExited(Windows::Foundation::IInspectable const &sender, DUX::Input::PointerRoutedEventArgs const &e);
		DUX::UIElement::PointerWheelChanged_revoker _imageTargetPointerWheelChangedRevoker{};
		void ImageTarget_PointerWheelChanged(Windows::Foundation::IInspectable const &sender, DUX::Input::PointerRoutedEventArgs const &e);
		DUX::UIElement::DragEnter_revoker _imageTargetDragEnterRevoker{};
		void ImageTarget_DragEnter(Windows::Foundation::IInspectable const &sender, DUX::DragEventArgs const &e);
		DUX::UIElement::DragOver_revoker _imageTargetDragOverRevoker{};
		void ImageTarget_DragOver(Windows::Foundation::IInspectable const &sender, DUX::DragEventArgs const &e);
		DUX::UIElement::DragLeave_revoker _imageTargetDragLeaveRevoker{};
		void ImageTarget_DragLeave(Windows::Foundation::IInspectable const &sender, DUX::DragEventArgs const &e);
		DUX::UIElement::Drop_revoker _imageTargetDropRevoker{};
		winrt::fire_and_forget ImageTarget_Drop(Windows::Foundation::IInspectable sender, DUX::DragEventArgs e);
		DUX::UIElement::DragStarting_revoker _imageTargetDragStartingRevoker{};
		void ImageTarget_DragStarting(DUX::UIElement const &sender, DUX::DragStartingEventArgs const &e);
		DUX::UIElement::ContextRequested_revoker _imageTargetContextRequestedRevoker{};
		void ImageTarget_ContextRequested(DUX::UIElement const &sender, DUX::Input::ContextRequestedEventArgs const &e);
		void ContextMenuItem_Click(Windows::Foundation::IInspectable const &sender, DUX::RoutedEventArgs const &e);
		DUXC::Primitives::ScrollBar::Scroll_revoker _horizontalScrollBarScrollRevoker{};
		void HorizontalScrollBar_Scroll(Windows::Foundation::IInspectable const &sender, DUX::Controls::Primitives::ScrollEventArgs const &e);
		DUXC::Primitives::ScrollBar::Scroll_revoker _verticalScrollBarScrollRevoker{};
		void VerticalScrollBar_Scroll(Windows::Foundation::IInspectable const &sender, DUX::Controls::Primitives::ScrollEventArgs const &e);
		void EditorBaseControl_CharacterReceived(DUX::UIElement const &sender, DUX::Input::CharacterReceivedRoutedEventArgs const &args);
		void OnLoaded(Windows::Foundation::IInspectable const &sender, DUX::RoutedEventArgs const &args);
		void OnUnloaded(Windows::Foundation::IInspectable const &sender, DUX::RoutedEventArgs const &args);
		void OnLosingFocus(Windows::Foundation::IInspectable const &sender, DUX::Input::LosingFocusEventArgs const &args);
		bool IsLoadedCompat();
		DUX::XamlRoot::Changed_revoker _xamlRootChangedRevoker{};
		void XamlRoot_Changed(DUX::XamlRoot const &sender, DUX::XamlRootChangedEventArgs const &args);
		void UpdateDpi(float dpiScale);
		void AddContextMenuItems(DUX::Controls::MenuFlyout const &menu);
		bool ShowContextMenu(DUX::UIElement const &targetElement, Windows::Foundation::Point const &point);
		bool ShowContextMenuAtCurrentPosition();
		winrt::com_ptr<::IVirtualSurfaceImageSourceNative> _vsisNative;
		std::shared_ptr<::WinUIEditor::MainWrapper> _wrapper{ nullptr };
		static LRESULT WndProc(Windows::Foundation::IInspectable const &, UINT msg, WPARAM wParam, LPARAM lParam);
		Windows::Graphics::Display::DisplayInformation::DpiChanged_revoker _dpiChangedRevoker{};
		void DisplayInformation_DpiChanged(Windows::Graphics::Display::DisplayInformation const &sender, Windows::Foundation::IInspectable const &args);
		Windows::UI::Xaml::Application::Suspending_revoker _suspendingRevoker{};
		void Application_Suspending(Windows::Foundation::IInspectable const &sender, Windows::ApplicationModel::SuspendingEventArgs const &args);
		void LoadLocalizedContextMenuResources();
		bool _localizedContextMenuResourcesLoaded{ false };
		winrt::hstring _textCommandLabelUndo;
		winrt::hstring _textCommandLabelRedo;
		winrt::hstring _textCommandLabelCut;
		winrt::hstring _textCommandLabelCopy;
		winrt::hstring _textCommandLabelPaste;
		winrt::hstring _textCommandLabelSelectAll;
		winrt::hstring _textCommandLabelDelete;
	};
}

namespace winrt::WinUIEditor::factory_implementation
{
	struct EditorBaseControl : EditorBaseControlT<EditorBaseControl, implementation::EditorBaseControl>
	{
	};
}
