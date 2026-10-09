// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "EditorBaseControl.h"
#if __has_include("EditorBaseControl.g.cpp")
#include "EditorBaseControl.g.cpp"
#endif
#include "EditorWrapper.h"
#include "NativeJournal.h"
#include "Helpers.h"
#include "EditorBaseControlAutomationPeer.h"
#include "MuiResourceLoader.h"
#include "XamlStringIds.h"
#include <exception>

using namespace ::WinUIEditor;
using namespace winrt;
using namespace DUX;
using namespace DUX::Automation::Peers;
using namespace DUX::Controls;
using namespace DUX::Controls::Primitives;
using namespace DUX::Input;
using namespace DUX::Media;
using namespace DUX::Media::Imaging;
using namespace Windows::Foundation;
using namespace Windows::Foundation::Collections;
using namespace Windows::System;
using namespace Windows::Graphics::Display;
using namespace Windows::ApplicationModel;
using namespace Windows::ApplicationModel::DataTransfer;

namespace winrt::WinUIEditor::implementation
{
	// Todo: Something about this control is keeping the WinUI 3 versions from closing.
	// Note that making this into a blank control fixes the issue, so it is definitely something here.
	EditorBaseControl::EditorBaseControl()
	{
		DefaultStyleKey(winrt::box_value(L"WinUIEditor.EditorBaseControl"));

		_wrapper = std::make_shared<MainWrapper>(*this);

		Loaded({ this, &EditorBaseControl::OnLoaded });
		Unloaded({ this, &EditorBaseControl::OnUnloaded });
		LosingFocus({ this, &EditorBaseControl::OnLosingFocus });

		if (!_hasXamlRoot)
		{
			_displayInformation = DisplayInformation::GetForCurrentView();
		}

		_scintilla = make_self<Scintilla::Internal::ScintillaWinUI>(_wrapper);
		_call = std::make_shared<Scintilla::ScintillaCall>();
		_call->SetFnPtr(reinterpret_cast<SciFnDirectStatus>(_scintilla->WndProc(Scintilla::Message::GetDirectStatusFunction, 0, 0)), _scintilla->WndProc(Scintilla::Message::GetDirectPointer, 0, 0));

		_editorWrapper = make<implementation::Editor>(get_strong());

		_scintilla->SetWndProcTag(*this);
		_scintilla->SetWndProc(&EditorBaseControl::WndProc);

		auto queue{ _hasFcu ? Windows::System::DispatcherQueue::GetForCurrentThread() : nullptr };
		if (const auto window{ Windows::UI::Core::CoreWindow::GetForCurrentThread() })
		{
			_coreWindowClosedRevoker = window.Closed(auto_revoke,
				[this](auto const &, auto const &) { FinalizeUIResources(); });
		}
		if (queue)
		{
			// The callback does not retain the control. final_release keeps its
			// storage alive until owning-thread destruction has completed.
			_shutdownStartingRevoker = queue.ShutdownStarting(auto_revoke,
				[this](auto const &, auto const &) { FinalizeUIResources(); });
		}

		if (_hasFcu)
		{
			// Registering the CharacterReceived event causes undesirable behavior with TSF in CoreWindow (normal UWP)
			// but is required to get text in classic windows (XAML Islands)
			// Todo: Find more ideal way to do this
			// Tried using _tfThreadManager->GetActiveFlags but TF_TMF_IMMERSIVEMODE flag was not accurate
			if (IsClassicWindow())
			{
				CharacterReceived({ this, &EditorBaseControl::EditorBaseControl_CharacterReceived });
			}
		}
	}

	EditorBaseControl::~EditorBaseControl()
	{
		FinalizeUIResources();
	}

	void EditorBaseControl::FinalizeUIResources()
	{
		if (!_uiResourcesFinalized.load())
		{
			_scintilla->Finalize();
			_uiResourcesFinalized.store(true);
		}
	}

	void EditorBaseControl::FinalizeAfterApartmentClosed() noexcept
	{
		if (!_uiResourcesFinalized.load())
		{
			// An apartment-disconnection error proves that owning-thread
			// callbacks can no longer run. Retire their native references even
			// if the host did not deliver its normal shutdown notification.
			_scintilla->FinalizeAfterApartmentClosed();
			_uiResourcesFinalized.store(true);
		}
	}

	winrt::fire_and_forget EditorBaseControl::final_release(std::unique_ptr<EditorBaseControl> self) noexcept
	{
		if (::GetCurrentThreadId() == self->_owningThreadId) co_return;
		try
		{
			// Release must never block a managed finalizer waiting for the UI.
			co_await winrt::resume_background();
			co_await self->_owningApartment;
		}
		catch (winrt::hresult_error const &error)
		{
			if (!::WinUIEditor::IsApartmentDisconnected(error.code())) std::terminate();
			self->FinalizeAfterApartmentClosed();
		}
	}

	WinUIEditor::Editor EditorBaseControl::Editor()
	{
		return _editorWrapper;
	}

	void EditorBaseControl::SetBackgroundColor(Windows::UI::Color const &color)
	{
		const Scintilla::Internal::ColourRGBA background{ color.R, color.G, color.B, color.A };
		_scintilla->StyleSetBackTransparent(Scintilla::Internal::StyleDefault, background);
		_scintilla->StyleSetBackTransparent(Scintilla::Internal::StyleLineNumber, background);
		_scintilla->SetFoldMarginColorTransparent(true, background);
		_scintilla->SetFoldMarginHiColorTransparent(true, background);
		_scintilla->PublicInvalidateStyleRedraw();
	}

	void EditorBaseControl::PasteText(hstring const &text)
	{
		_scintilla->PasteText(std::wstring_view(text));
	}

	void EditorBaseControl::AcquireReadLease() { _scintilla->AcquireReadLease(); }
	void EditorBaseControl::ReleaseReadLease() noexcept { _scintilla->ReleaseReadLease(); }
	void EditorBaseControl::ReplaceDocumentWithUndo(Scintilla::Internal::Document &replacement) { _scintilla->ReplaceDocumentWithUndo(replacement); }
	void EditorBaseControl::SetLoadedDocument(Scintilla::Internal::Document &replacement)
	{
		// Loading replaces content, while the editor's indentation preferences
		// remain in effect. Scintilla stores these options on each document.
		const auto tabWidth = _call->TabWidth();
		const auto indent = _call->Indent();
		const auto useTabs = _call->UseTabs();
		const auto tabIndents = _call->TabIndents();
		const auto backspaceUnindents = _call->BackSpaceUnIndents();
		_call->SetDocPointer(replacement.AsDocumentEditable());
		// Use Scintilla's setters to update derived indentation and layout state.
		_call->SetTabWidth(tabWidth);
		_call->SetIndent(indent);
		_call->SetUseTabs(useTabs);
		_call->SetTabIndents(tabIndents);
		_call->SetBackSpaceUnIndents(backspaceUnindents);
	}
	void EditorBaseControl::PublishPreparedReplacement(Scintilla::Internal::Document &replacement, const char *previousText,
		std::unique_ptr<Scintilla::Internal::IContractionState> contraction, bool leased)
	{
		_scintilla->PublishPreparedReplacement(replacement, previousText, std::move(contraction), leased);
	}
	const Scintilla::Internal::Document &EditorBaseControl::LeasedDocument() const { return _scintilla->LeasedDocument(); }
	void EditorBaseControl::SetDisplayLineMap(std::vector<Scintilla::Internal::DisplayGap> gaps, std::vector<Scintilla::Internal::TintedLineRange> tints)
	{
		_scintilla->SetDisplayLineMap(std::move(gaps), std::move(tints));
	}
	void EditorBaseControl::SetDisplayColours(unsigned int line, unsigned int gap, unsigned int hatch) { _scintilla->SetDisplayColours(line, gap, hatch); }
	void EditorBaseControl::SetDisplayHighlight(int64_t row, int64_t count, unsigned int colour) { _scintilla->SetDisplayHighlight(row, count, colour); }
	std::shared_ptr<::WinUIEditor::NativeDocumentJournal> EditorBaseControl::PrepareJournalReplacement() { return _scintilla->PrepareJournalReplacement(); }
	uint64_t EditorBaseControl::DocumentSequence() const noexcept { return _scintilla->DocumentSequence(); }
	uint64_t EditorBaseControl::DocumentRevision() const noexcept { return _scintilla->DocumentRevision(); }
	uint64_t EditorBaseControl::SelectionRevision() const noexcept { return _scintilla->SelectionRevision(); }
	bool EditorBaseControl::HasJournal() const noexcept { return _scintilla->HasJournal(); }
	Windows::Foundation::IAsyncAction EditorBaseControl::WaitForTextStoreUnlockAsync()
	{
		auto lifetime = get_strong();
		while (_scintilla->HasTextStoreLock())
		{
			co_await resume_foreground(Dispatcher());
		}
	}
	void EditorBaseControl::StartJournal(std::shared_ptr<::WinUIEditor::NativeJournalFile> file, uint64_t sequence, bool rotate) { _scintilla->StartJournal(std::move(file), sequence, rotate); }
	::WinUIEditor::JournalPrefix EditorBaseControl::AcquireJournalPrefix() { return _scintilla->AcquireJournalPrefix(); }
	std::shared_ptr<::WinUIEditor::NativeJournalFile> EditorBaseControl::ActiveJournalFile() const noexcept { return _scintilla->ActiveJournalFile(); }
	std::vector<std::shared_ptr<::WinUIEditor::NativeJournalFile>> EditorBaseControl::StopJournal() { return _scintilla->StopJournal(); }

	void EditorBaseControl::OnLoaded(IInspectable const &sender, DUX::RoutedEventArgs const &args)
	{
		// Following pattern from https://github.com/microsoft/microsoft-ui-xaml/blob/a7183df20367bc0e2b8c825430597a5c1e6871b6/dev/WebView2/WebView2.cpp#L1556

		_isLoaded = true;

		if (!IsLoadedCompat())
		{
			return;
		}
		_scintilla->ResumeIdleWork();

		if (_hasXamlRoot)
		{
			UpdateDpi(XamlRoot().RasterizationScale());
			_xamlRootChangedRevoker = XamlRoot().Changed(auto_revoke, { this, &EditorBaseControl::XamlRoot_Changed });
		}
		else
		{
			UpdateDpi(_displayInformation.RawPixelsPerViewPixel());
			_dpiChangedRevoker = _displayInformation.DpiChanged(auto_revoke, { this, &EditorBaseControl::DisplayInformation_DpiChanged });
		}
	}

	void EditorBaseControl::OnUnloaded(IInspectable const &sender, DUX::RoutedEventArgs const &args)
	{
		_isLoaded = false;

		if (IsLoadedCompat())
		{
			return;
		}

		if (_hasXamlRoot)
		{
			_xamlRootChangedRevoker.revoke();
		}
		else
		{
			_dpiChangedRevoker.revoke();
		}

		_scintilla->StopTimers();

		_wrapper->ReleaseAutocompletePopup();
	}

	void EditorBaseControl::OnLosingFocus(Windows::Foundation::IInspectable const &sender, DUX::Input::LosingFocusEventArgs const &args)
	{
		const auto newFocusedElement{ args.NewFocusedElement() };
		if (!newFocusedElement || !_call->AutoCActive())
		{
			return;
		}

		DependencyObject object{ newFocusedElement };
		do
		{
			if (object.try_as<AutocompletionControl>())
			{
				args.Cancel(true);
				return;
			}
			object = VisualTreeHelper::GetParent(object);
		} while (object);
	}

	bool EditorBaseControl::IsLoadedCompat()
	{
		if (_hasIsLoaded)
		{
			return IsLoaded();
		}
		else
		{
			return _isLoaded;
		}
	}

	void EditorBaseControl::DisplayInformation_DpiChanged(DisplayInformation const &sender, IInspectable const &args)
	{
		UpdateDpi(sender.RawPixelsPerViewPixel());
	}

	void EditorBaseControl::XamlRoot_Changed(DUX::XamlRoot const &sender, XamlRootChangedEventArgs const &args)
	{
		UpdateDpi(sender.RasterizationScale());
	}

	void EditorBaseControl::UpdateDpi(float dpiScale)
	{
		if (_dpiScale != dpiScale)
		{
			_wrapper->LogicalDpi(dpiScale * 96);
			_dpiScale = dpiScale;
			_scintilla->DpiChanged();
			_dpiChangedEvent(*this, dpiScale);
		}
	}

	void EditorBaseControl::LoadLocalizedContextMenuResources()
	{
		// Find Windows.UI.Xaml.dll or Microsoft.ui.xaml.dll by pointer address of vtable
		// Not using name because Microsoft.ui.xaml.dll could be ambiguous for a process
		// that loads both WinUI 2 and WinUI 3

		const MuiResourceLoader xamlMuiResourceLoader{ this->try_as<IInspectable>() };

		_textCommandLabelUndo = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelUndo, L"Undo");
		_textCommandLabelRedo = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelRedo, L"Redo");
		_textCommandLabelCut = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelCut, L"Cut");
		_textCommandLabelCopy = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelCopy, L"Copy");
		_textCommandLabelPaste = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelPaste, L"Paste");
		_textCommandLabelSelectAll = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelSelectAll, L"Select All");
		_textCommandLabelDelete = xamlMuiResourceLoader.GetString(XamlTextCommand::LabelDelete, L"Delete");
	}

	void EditorBaseControl::AddContextMenuItems(MenuFlyout const &menu)
	{
		if (!_localizedContextMenuResourcesLoaded)
		{
			LoadLocalizedContextMenuResources();
			_localizedContextMenuResourcesLoaded = true;
		}

		const auto writable{ !static_cast<bool>(_scintilla->WndProc(Scintilla::Message::GetReadOnly, 0, 0)) };
		const auto selection{ !static_cast<bool>(_scintilla->WndProc(Scintilla::Message::GetSelectionEmpty, 0, 0)) };

		const MenuFlyoutItem undoItem{};
		undoItem.Text(_textCommandLabelUndo);
		undoItem.Icon(SymbolIcon{ Symbol::Undo });
		undoItem.Tag(box_value(ScintillaMessage::Undo));
		undoItem.IsEnabled(_scintilla->WndProc(Scintilla::Message::CanUndo, 0, 0));
		undoItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click }); // Todo: Revoke event handler?
		menu.Items().Append(undoItem);

		const MenuFlyoutItem redoItem{};
		redoItem.Text(_textCommandLabelRedo);
		redoItem.Icon(SymbolIcon{ Symbol::Redo });
		redoItem.Tag(box_value(ScintillaMessage::Redo));
		redoItem.IsEnabled(_scintilla->WndProc(Scintilla::Message::CanRedo, 0, 0));
		redoItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click });
		menu.Items().Append(redoItem);

		menu.Items().Append(MenuFlyoutSeparator{});

		const MenuFlyoutItem cutItem{};
		cutItem.Text(_textCommandLabelCut);
		cutItem.Icon(SymbolIcon{ Symbol::Cut });
		cutItem.Tag(box_value(ScintillaMessage::Cut));
		cutItem.IsEnabled(writable && selection);
		cutItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click });
		menu.Items().Append(cutItem);

		const MenuFlyoutItem copyItem{};
		copyItem.Text(_textCommandLabelCopy);
		copyItem.Icon(SymbolIcon{ Symbol::Copy });
		copyItem.Tag(box_value(ScintillaMessage::Copy));
		copyItem.IsEnabled(selection);
		copyItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click });
		menu.Items().Append(copyItem);

		const MenuFlyoutItem pasteItem{};
		pasteItem.Text(_textCommandLabelPaste);
		pasteItem.Icon(SymbolIcon{ Symbol::Paste });
		pasteItem.Tag(box_value(ScintillaMessage::Paste));
		pasteItem.IsEnabled(_scintilla->WndProc(Scintilla::Message::CanPaste, 0, 0));
		pasteItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click });
		menu.Items().Append(pasteItem);

		const MenuFlyoutItem deleteItem{};
		deleteItem.Text(_textCommandLabelDelete);
		deleteItem.Icon(SymbolIcon{ Symbol::Delete });
		deleteItem.Tag(box_value(ScintillaMessage::Clear));
		deleteItem.IsEnabled(writable && selection);
		deleteItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click });
		menu.Items().Append(deleteItem);

		menu.Items().Append(MenuFlyoutSeparator{});

		const MenuFlyoutItem selectAllItem{};
		selectAllItem.Text(_textCommandLabelSelectAll);
		selectAllItem.Icon(SymbolIcon{ Symbol::SelectAll });
		selectAllItem.Tag(box_value(ScintillaMessage::SelectAll));
		selectAllItem.Click({ this, &EditorBaseControl::ContextMenuItem_Click });
		menu.Items().Append(selectAllItem);
	}

	bool EditorBaseControl::ShowContextMenu(UIElement const &targetElement, Point const &point)
	{
		if (_scintilla->ShouldShowContextMenu(Point{ point.X * _dpiScale, point.Y * _dpiScale }))
		{
			// Hosts can supply their own editor menu with app-specific commands.
			if (const auto hostMenu{ ContextFlyout().try_as<MenuFlyout>() })
			{
				hostMenu.ShowAt(targetElement, point);
				return true;
			}

			const MenuFlyout menu{};
			AddContextMenuItems(menu);
			menu.ShowAt(targetElement, point);
			return true;
		}
		return false;
	}

	bool EditorBaseControl::ShowContextMenuAtCurrentPosition()
	{
		if (auto imageTarget{ GetTemplateChild(L"ImageTarget").try_as<UIElement>() }) // Todo: Store this
		{
			const auto pos{ _scintilla->WndProc(Scintilla::Message::GetCurrentPos, 0, 0) };
			return ShowContextMenu(imageTarget, Point{
				_scintilla->WndProc(Scintilla::Message::PointXFromPosition, 0, pos) / _dpiScale,
				(_scintilla->WndProc(Scintilla::Message::PointYFromPosition, 0, pos) + _scintilla->WndProc(Scintilla::Message::TextHeight, 0, 0)) / _dpiScale
				});
		}
		return false;
	}

	Scintilla::sptr_t EditorBaseControl::PublicWndProc(Scintilla::Message iMessage, Scintilla::uptr_t wParam, Scintilla::sptr_t lParam)
	{
		return _scintilla->WndProc(iMessage, wParam, lParam);
	}

	int64_t EditorBaseControl::SendMessage(ScintillaMessage const &message, uint64_t wParam, int64_t lParam)
	{
		return PublicWndProc(static_cast<Scintilla::Message>(message), wParam, lParam);
	}

	void EditorBaseControl::StyleSetForeTransparent(int style, Scintilla::Internal::ColourRGBA color)
	{
		_scintilla->StyleSetForeTransparent(style, color);
	}

	void EditorBaseControl::StyleSetBackTransparent(int style, Scintilla::Internal::ColourRGBA color)
	{
		_scintilla->StyleSetBackTransparent(style, color);
	}

	void EditorBaseControl::InvalidateStyleRedraw()
	{
		_scintilla->PublicInvalidateStyleRedraw();
	}

	void EditorBaseControl::StyleClearCustom()
	{
		_scintilla->StyleClearCustom();
	}

	void EditorBaseControl::SetFoldMarginColorTransparent(bool useSetting, Scintilla::Internal::ColourRGBA back)
	{
		_scintilla->SetFoldMarginColorTransparent(useSetting, back);
	}

	void EditorBaseControl::SetFoldMarginHiColorTransparent(bool useSetting, Scintilla::Internal::ColourRGBA fore)
	{
		_scintilla->SetFoldMarginHiColorTransparent(useSetting, fore);
	}

	void EditorBaseControl::SetCallTipBackgroundColorTransparent(Scintilla::Internal::ColourRGBA color)
	{
		_scintilla->SetCallTipBackgroundColorTransparent(color);
	}

	void EditorBaseControl::SetCallTipHoverColor(Scintilla::Internal::ColourRGBA color)
	{
		_scintilla->SetCallTipHoverColor(color);
	}

	void EditorBaseControl::OnApplyTemplate()
	{
		__super::OnApplyTemplate();

		if (_hasXamlRoot)
		{
			UpdateDpi(XamlRoot().RasterizationScale());
		}
		else
		{
			UpdateDpi(_displayInformation.RawPixelsPerViewPixel());
		}

		const VirtualSurfaceImageSource virtualSurfaceImageSource{ 0, 0, false };

		_vsisNative = virtualSurfaceImageSource.as<::IVirtualSurfaceImageSourceNative>();

		_wrapper->VsisNative(_vsisNative);
		_wrapper->CreateGraphicsDevices();
		_vsisNative->RegisterForUpdatesNeeded(_scintilla.as<::IVirtualSurfaceUpdatesCallbackNative>().get());

		// The SurfaceImageSource object's underlying
		// ISurfaceImageSourceNativeWithD2D object will contain the completed bitmap.

		const auto horizontalScrollBar{ GetTemplateChild(L"HorizontalScrollBar").try_as<ScrollBar>() };
		const auto verticalScrollBar{ GetTemplateChild(L"VerticalScrollBar").try_as<ScrollBar>() };
		if (horizontalScrollBar)
		{
			_horizontalScrollBarScrollRevoker = horizontalScrollBar.Scroll(auto_revoke, { this, &EditorBaseControl::HorizontalScrollBar_Scroll });
		}
		if (verticalScrollBar)
		{
			_verticalScrollBarScrollRevoker = verticalScrollBar.Scroll(auto_revoke, { this, &EditorBaseControl::VerticalScrollBar_Scroll });
		}
		_wrapper->SetScrollBars(horizontalScrollBar, verticalScrollBar);

		if (const auto imageTarget{ GetTemplateChild(L"ImageTarget").try_as<Border>() })
		{
			_imageTargetSizeChangedRevoker = imageTarget.SizeChanged(auto_revoke, { this, &EditorBaseControl::ImageTarget_SizeChanged });
			_imageTargetPointerMovedRevoker = imageTarget.PointerMoved(auto_revoke, { this, &EditorBaseControl::ImageTarget_PointerMoved });
			_imageTargetPointerCaptureLostRevoker = imageTarget.PointerCaptureLost(auto_revoke, { this, &EditorBaseControl::ImageTarget_PointerCaptureLost });
			_imageTargetPointerEnteredRevoker = imageTarget.PointerEntered(auto_revoke, { this, &EditorBaseControl::ImageTarget_PointerEntered });
			_imageTargetPointerExitedRevoker = imageTarget.PointerExited(auto_revoke, { this, &EditorBaseControl::ImageTarget_PointerExited });
			_imageTargetPointerWheelChangedRevoker = imageTarget.PointerWheelChanged(auto_revoke, { this, &EditorBaseControl::ImageTarget_PointerWheelChanged });
			_imageTargetDragEnterRevoker = imageTarget.DragEnter(auto_revoke, { this, &EditorBaseControl::ImageTarget_DragEnter });
			_imageTargetDragOverRevoker = imageTarget.DragOver(auto_revoke, { this, &EditorBaseControl::ImageTarget_DragOver });
			_imageTargetDragLeaveRevoker = imageTarget.DragLeave(auto_revoke, { this, &EditorBaseControl::ImageTarget_DragLeave });
			_imageTargetDropRevoker = imageTarget.Drop(auto_revoke, { this, &EditorBaseControl::ImageTarget_Drop });

			_wrapper->SetMouseCaptureElement(imageTarget);
			_imageTargetDragStartingRevoker = imageTarget.DragStarting(auto_revoke, { this, &EditorBaseControl::ImageTarget_DragStarting });

			_imageTargetContextRequestedRevoker = imageTarget.ContextRequested(auto_revoke, { this, &EditorBaseControl::ImageTarget_ContextRequested });

			const ImageBrush brush{};
			brush.ImageSource(virtualSurfaceImageSource);
			imageTarget.Background(brush);
		}

		// Todo: Evaluate if this is an appropriate place to add this event (and other code in this method)
		_suspendingRevoker = Application::Current().Suspending(auto_revoke, { this, &EditorBaseControl::Application_Suspending });
	}

	// Todo: Focus bug: deactive window, click on control, press ctrl+a quickly. result: selection disappears

	void EditorBaseControl::OnGotFocus(RoutedEventArgs const &e)
	{
		__super::OnGotFocus(e);

		_isFocused = true;

		_scintilla->FocusChanged(true);
	}

	void EditorBaseControl::OnLostFocus(RoutedEventArgs const &e)
	{
		__super::OnLostFocus(e);

		_isFocused = false;

		_scintilla->FocusChanged(false);

		if (!_call->AutoCActive())
		{
			_wrapper->ReleaseAutocompletePopup();
		}
	}

	void EditorBaseControl::OnPointerPressed(DUX::Input::PointerRoutedEventArgs const &e)
	{
		__super::OnPointerPressed(e);

		Focus(FocusState::Pointer);

		// The focus state seems to get confused if the following code is placed in ImageTarget_PointerPressed.
		// OnPointerReleased is the same for symmetry. ImageTarget_PointerMoved is there to show correct cursor
		// when moving the cursor over the scrollbar (UWP).

		if (auto imageTarget{ GetTemplateChild(L"ImageTarget").try_as<UIElement>() }) // Todo: Store this
		{
			auto point{ e.GetCurrentPoint(imageTarget) };
			auto scaled{ point.Position() };
			switch (point.Properties().PointerUpdateKind())
			{
			case DUI::PointerUpdateKind::LeftButtonPressed:
				_scintilla->PointerPressed(Point{ scaled.X * _dpiScale, scaled.Y * _dpiScale }, point.Timestamp() / 1000ul, e.KeyModifiers());
				break;
			case DUI::PointerUpdateKind::RightButtonPressed:
				_scintilla->RightPointerPressed(Point{ scaled.X * _dpiScale, scaled.Y * _dpiScale }, point.Timestamp() / 1000ul, e.KeyModifiers());
				break;
			}
			// Todo: make sure the loss of precision is not going to realistically cause a problem
		}
	}

	void EditorBaseControl::OnPointerReleased(DUX::Input::PointerRoutedEventArgs const &e)
	{
		__super::OnPointerReleased(e);

		if (auto imageTarget{ GetTemplateChild(L"ImageTarget").try_as<UIElement>() }) // Todo: Store this
		{
			auto point{ e.GetCurrentPoint(imageTarget) };
			auto scaled{ point.Position() };
			switch (point.Properties().PointerUpdateKind())
			{
			case DUI::PointerUpdateKind::LeftButtonReleased:
				_scintilla->PointerReleased(Point{ scaled.X * _dpiScale, scaled.Y * _dpiScale }, point.Timestamp() / 1000ul, e.KeyModifiers());
				break;
			}
		}

		e.Handled(true); // Prevents control from losing focus on pointer released
		// See https://stackoverflow.com/questions/59392044/uwp-control-to-keep-focus-after-mouse-click
		// Alternate approach: call Focus in OnFocusLost
	}

	AutomationPeer EditorBaseControl::OnCreateAutomationPeer()
	{
		return make<EditorBaseControlAutomationPeer>(*this);
	}

	void EditorBaseControl::ImageTarget_PointerMoved(IInspectable const &sender, PointerRoutedEventArgs const &e)
	{
		if (auto imageTarget{ GetTemplateChild(L"ImageTarget").try_as<UIElement>() }) // Todo: Store this
		{
			auto point{ e.GetCurrentPoint(imageTarget) };
			auto scaled{ point.Position() };
			auto x{ scaled.X * _dpiScale };
			auto y{ scaled.Y * _dpiScale };

			_scintilla->PointerMoved(Point{ x, y }, point.Timestamp() / 1000ul, e.KeyModifiers(), point);
		}
	}

	void EditorBaseControl::ImageTarget_PointerCaptureLost(IInspectable const &sender, PointerRoutedEventArgs const &e)
	{
		if (!_isPointerOver)
		{
			// Todo: if you, e.g. hover over a HyperlinkButton when this is called, you will get an arrow instead of the hand you want
			winrt::Windows::UI::Core::CoreWindow::GetForCurrentThread().PointerCursor(Windows::UI::Core::CoreCursor{ Windows::UI::Core::CoreCursorType::Arrow, 0 });
		}
	}

	void EditorBaseControl::ImageTarget_PointerEntered(IInspectable const &sender, PointerRoutedEventArgs const &e)
	{
		_isPointerOver = true;
	}

	void EditorBaseControl::ImageTarget_PointerExited(IInspectable const &sender, PointerRoutedEventArgs const &e)
	{
		_isPointerOver = false;
		if (!_wrapper->HaveMouseCapture())
		{
			winrt::Windows::UI::Core::CoreWindow::GetForCurrentThread().PointerCursor(Windows::UI::Core::CoreCursor{ Windows::UI::Core::CoreCursorType::Arrow, 0 });
		}
		_scintilla->PointerExited();
	}

	void EditorBaseControl::OnKeyDown(KeyRoutedEventArgs const &e)
	{
		__super::OnKeyDown(e);

		const auto modifiers{ GetKeyModifiersForCurrentThread() };

		auto handled = true;
		_scintilla->KeyDown(e.Key(), modifiers, e.KeyStatus().IsExtendedKey, &handled); // Todo: Or use VirtualKey?

		if (!handled
			&& e.Key() == VirtualKey::F10 && (modifiers & VirtualKeyModifiers::Shift) == VirtualKeyModifiers::Shift
			&& ShowContextMenuAtCurrentPosition())
		{
			handled = true;
		}

		e.Handled(handled);
	}

	void EditorBaseControl::OnKeyUp(KeyRoutedEventArgs const &e)
	{
		if (e.Key() == VirtualKey::Application)
		{
			e.Handled(ShowContextMenuAtCurrentPosition());
		}
	}

	void EditorBaseControl::EditorBaseControl_CharacterReceived(DUX::UIElement const &sender, CharacterReceivedRoutedEventArgs const &e)
	{
		if (_isFocused)
		{
			_scintilla->CharacterReceived(e.Character());
			e.Handled(true);
		}
	}

	void EditorBaseControl::ImageTarget_SizeChanged(IInspectable const &sender, SizeChangedEventArgs const &args)
	{
		if (_vsisNative)
		{
			auto width{ ConvertFromDipToPixelUnit(args.NewSize().Width, _dpiScale) };
			auto height{ ConvertFromDipToPixelUnit(args.NewSize().Height, _dpiScale) };
			_wrapper->Width(width);
			_wrapper->Height(height);
			_vsisNative->Resize(width, height);
			_scintilla->SizeChanged();
		}
	}

	void EditorBaseControl::ImageTarget_PointerWheelChanged(IInspectable const &sender, PointerRoutedEventArgs const &e)
	{
		if (e.Handled())
			return;
		auto properties{ e.GetCurrentPoint(sender.as<UIElement>()).Properties() };
		const auto modifiers{ e.KeyModifiers() };
		if (!_mouseWheelZoomEnabled && !properties.IsHorizontalMouseWheel() &&
			(static_cast<uint32_t>(modifiers) & static_cast<uint32_t>(VirtualKeyModifiers::Control)) &&
			!(static_cast<uint32_t>(modifiers) & static_cast<uint32_t>(VirtualKeyModifiers::Shift)))
		{
			// Leave the event unhandled so the host can apply its own zoom policy.
			return;
		}
		_scintilla->PointerWheelChanged(properties.MouseWheelDelta(), properties.IsHorizontalMouseWheel(), e.KeyModifiers());
	}

	void EditorBaseControl::ImageTarget_DragEnter(IInspectable const &sender, DragEventArgs const &e)
	{
		DataPackageOperation op;
		_scintilla->DragEnter(e.DataView(), e.AllowedOperations(), e.Modifiers(), op);
		e.AcceptedOperation(op);
		e.DragUIOverride().IsContentVisible(false);
		e.DragUIOverride().IsCaptionVisible(false);
	}

	void EditorBaseControl::ImageTarget_DragOver(IInspectable const &sender, DragEventArgs const &e)
	{
		auto point{ e.GetPosition(sender.as<UIElement>()) };
		point.X *= _dpiScale;
		point.Y *= _dpiScale;
		DataPackageOperation op;
		_scintilla->DragOver(point, e.AllowedOperations(), e.Modifiers(), op);
		e.AcceptedOperation(op);
	}

	void EditorBaseControl::ImageTarget_DragLeave(IInspectable const &sender, DragEventArgs const &e)
	{
		_scintilla->DragLeave();
	}

	void EditorBaseControl::ImageTarget_Drop(IInspectable const &sender, DragEventArgs const &e)
	{
		auto point{ e.GetPosition(sender.as<UIElement>()) };
		point.X *= _dpiScale;
		point.Y *= _dpiScale;
		DataPackageOperation op;
		_scintilla->Drop(point, e.DataView(), e.AllowedOperations(), e.Modifiers(), op);
		e.AcceptedOperation(op);
	}

	void EditorBaseControl::ImageTarget_DragStarting(UIElement const &sender, DragStartingEventArgs const &e)
	{
		e.AllowedOperations(DataPackageOperation::Copy | DataPackageOperation::Move);
		e.Data().SetText(winrt::to_hstring(_scintilla->GetDragData()));
		e.DragUI().SetContentFromDataPackage();
	}

	void EditorBaseControl::ImageTarget_ContextRequested(UIElement const &sender, ContextRequestedEventArgs const &e)
	{
		Point point;
		if (e.TryGetPosition(sender, point))
		{
			e.Handled(ShowContextMenu(sender, point));
		}
		else if (const auto frameworkElement{ sender.try_as<FrameworkElement>() })
		{
			e.Handled(ShowContextMenuAtCurrentPosition());
		}
	}

	void EditorBaseControl::ContextMenuItem_Click(Windows::Foundation::IInspectable const &sender, DUX::RoutedEventArgs const &e)
	{
		_scintilla->WndProc(static_cast<Scintilla::Message>(unbox_value<ScintillaMessage>(sender.as<FrameworkElement>().Tag())), 0, 0);
	}

	void EditorBaseControl::HorizontalScrollBar_Scroll(IInspectable const &sender, ScrollEventArgs const &e)
	{
		_scintilla->HorizontalScroll(static_cast<Scintilla::Internal::ScrollEventType>(e.ScrollEventType()), static_cast<int>(e.NewValue()));
	}

	void EditorBaseControl::VerticalScrollBar_Scroll(IInspectable const &sender, ScrollEventArgs const &e)
	{
		_scintilla->Scroll(static_cast<Scintilla::Internal::ScrollEventType>(e.ScrollEventType()), static_cast<int>(e.NewValue()));
	}

	LRESULT EditorBaseControl::WndProc(IInspectable const &tag, UINT msg, WPARAM wParam, LPARAM lParam)
	{
		if (msg == WM_NOTIFY)
		{
			const auto data{ reinterpret_cast<Scintilla::NotificationData *>(lParam) };
			const auto sender{ tag.as<EditorBaseControl>() };
			sender->InternalNotifyMessageReceived(*sender, lParam); // Internal subscribers receive notifications before public subscriptions
			sender->_notifyMessageReceived(*sender, lParam);
			sender->_editorWrapper.as<implementation::Editor>()->ProcessEvent(data);
		}

		return 0;
	}

	event_token EditorBaseControl::DpiChanged(EventHandler<double> const &handler)
	{
		return _dpiChangedEvent.add(handler);
	}

	void EditorBaseControl::DpiChanged(event_token const &token) noexcept
	{
		_dpiChangedEvent.remove(token);
	}

	event_token EditorBaseControl::NotifyMessageReceived(EventHandler<int64_t> const &handler)
	{
		return _notifyMessageReceived.add(handler);
	}

	void EditorBaseControl::NotifyMessageReceived(event_token const &token) noexcept
	{
		_notifyMessageReceived.remove(token);
	}

	float EditorBaseControl::DpiScale()
	{
		return _dpiScale;
	}

	void EditorBaseControl::Application_Suspending(IInspectable const &sender, SuspendingEventArgs const &args)
	{
		// Required or crashes on resume
		// https://learn.microsoft.com/en-us/windows/uwp/gaming/directx-and-xaml-interop
		// https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_3/nf-dxgi1_3-idxgidevice3-trim
		// Todo: Should ClearResources get called too? https://github.com/microsoft/Win2D/blob/master/winrt/lib/drawing/CanvasDevice.cpp#L1040
		_wrapper->TrimDxgiDevice();
	}
}
