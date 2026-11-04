// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once

#include "CallTipControl.g.h"

#include "ScintillaWin.h"
#include "ScintillaCall.h"
#include "Wrapper.h"
#include "CallTipWrapper.h"

namespace winrt::WinUIEditor::implementation
{
	struct CallTipControl : CallTipControlT<CallTipControl>
	{
		CallTipControl(winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> const &scintilla);
		~CallTipControl();

		void OnApplyTemplate();
		void OnTapped(DUX::Input::TappedRoutedEventArgs const &e);
		void OnPointerMoved(DUX::Input::PointerRoutedEventArgs const &e);
		void OnPointerPressed(DUX::Input::PointerRoutedEventArgs const &e);
		void OnPointerReleased(DUX::Input::PointerRoutedEventArgs const &e);
		DUX::Automation::Peers::AutomationPeer OnCreateAutomationPeer();

		event<Windows::Foundation::EventHandler<int64_t>> InternalNotifyMessageReceived;

		std::shared_ptr<::WinUIEditor::CallTipWrapper> Wrapper();

	private:
		winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> _scintilla;
		DUXC::Border _imageTarget{ nullptr };
		bool _hasFcu{ Windows::Foundation::Metadata::ApiInformation::IsApiContractPresent(L"Windows.Foundation.UniversalApiContract", 5) }; // Todo: Make static
		bool _hasXamlRoot{ Windows::Foundation::Metadata::ApiInformation::IsTypePresent(L"Windows.UI.Xaml.XamlRoot") }; // Todo: Make static
		bool _hasIsLoaded{ Windows::Foundation::Metadata::ApiInformation::IsPropertyPresent(L"Windows.UI.Xaml.FrameworkElement", L"IsLoaded") }; // Todo: Make static
		bool _isPointerOver{ false };
		Windows::Graphics::Display::DisplayInformation _displayInformation{ nullptr };
		bool _isLoaded{ false };
		bool _isFocused{ false };
		float _dpiScale{ 0 };
		event<Windows::Foundation::EventHandler<double>> _dpiChangedEvent;
		event<Windows::Foundation::EventHandler<int64_t>> _notifyMessageReceived;
		DUX::FrameworkElement::SizeChanged_revoker _imageTargetSizeChangedRevoker{};
		void ImageTarget_SizeChanged(Windows::Foundation::IInspectable const &sender, DUX::SizeChangedEventArgs const &args);
		void OnLoaded(Windows::Foundation::IInspectable const &sender, DUX::RoutedEventArgs const &args);
		void OnUnloaded(Windows::Foundation::IInspectable const &sender, DUX::RoutedEventArgs const &args);
		bool IsLoadedCompat();
		DUX::XamlRoot::Changed_revoker _xamlRootChangedRevoker{};
		void XamlRoot_Changed(DUX::XamlRoot const &sender, DUX::XamlRootChangedEventArgs const &args);
		void UpdateDpi(float dpiScale);
		winrt::com_ptr<::IVirtualSurfaceImageSourceNative> _vsisNative;
		std::shared_ptr<::WinUIEditor::CallTipWrapper> _wrapper{ nullptr };
		Windows::Graphics::Display::DisplayInformation::DpiChanged_revoker _dpiChangedRevoker{};
		void DisplayInformation_DpiChanged(Windows::Graphics::Display::DisplayInformation const &sender, Windows::Foundation::IInspectable const &args);
		Windows::UI::Xaml::Application::Suspending_revoker _suspendingRevoker{};
		void Application_Suspending(Windows::Foundation::IInspectable const &sender, Windows::ApplicationModel::SuspendingEventArgs const &args);
	};
}
