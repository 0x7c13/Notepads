// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "CallTipControl.h"
#if __has_include("CallTipControl.g.cpp")
#include "CallTipControl.g.cpp"
#endif
#include "EditorWrapper.h"
#include "Helpers.h"
//#include "CallTipControlAutomationPeer.h"

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

namespace winrt::WinUIEditor::implementation
{
	CallTipControl::CallTipControl(winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> const &scintilla)
	{
		DefaultStyleKey(winrt::box_value(L"WinUIEditor.CallTipControl"));

		_scintilla = scintilla;

		_wrapper = std::make_shared<::WinUIEditor::CallTipWrapper>(*this);

		const Border imageTarget{};
		imageTarget.Name(L"ImageTarget");
		Content(imageTarget);

		Loaded({ this, &CallTipControl::OnLoaded });
		Unloaded({ this, &CallTipControl::OnUnloaded });

		if (!_hasXamlRoot)
		{
			_displayInformation = DisplayInformation::GetForCurrentView();
		}
	}

	CallTipControl::~CallTipControl()
	{
		if (_vsisNative)
		{
			_vsisNative->RegisterForUpdatesNeeded(nullptr);
		}
	}

	void CallTipControl::OnLoaded(IInspectable const &sender, DUX::RoutedEventArgs const &args)
	{
		// Following pattern from https://github.com/microsoft/microsoft-ui-xaml/blob/a7183df20367bc0e2b8c825430597a5c1e6871b6/dev/WebView2/WebView2.cpp#L1556

		_isLoaded = true;

		if (!IsLoadedCompat())
		{
			return;
		}

		if (_hasXamlRoot)
		{
			UpdateDpi(XamlRoot().RasterizationScale());
			_xamlRootChangedRevoker = XamlRoot().Changed(auto_revoke, { this, &CallTipControl::XamlRoot_Changed });
		}
		else
		{
			UpdateDpi(_displayInformation.RawPixelsPerViewPixel());
			_dpiChangedRevoker = _displayInformation.DpiChanged(auto_revoke, { this, &CallTipControl::DisplayInformation_DpiChanged });
		}
	}

	void CallTipControl::OnUnloaded(IInspectable const &sender, DUX::RoutedEventArgs const &args)
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
	}

	bool CallTipControl::IsLoadedCompat()
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

	void CallTipControl::DisplayInformation_DpiChanged(DisplayInformation const &sender, IInspectable const &args)
	{
		UpdateDpi(sender.RawPixelsPerViewPixel());
	}

	void CallTipControl::XamlRoot_Changed(DUX::XamlRoot const &sender, XamlRootChangedEventArgs const &args)
	{
		UpdateDpi(sender.RasterizationScale());
	}

	void CallTipControl::UpdateDpi(float dpiScale)
	{
		if (_dpiScale != dpiScale)
		{
			_wrapper->LogicalDpi(dpiScale * 96);
			_dpiScale = dpiScale;
		}
	}

	void CallTipControl::OnApplyTemplate()
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

		const VirtualSurfaceImageSource virtualSurfaceImageSource{ 0, 0 };

		_vsisNative = virtualSurfaceImageSource.as<::IVirtualSurfaceImageSourceNative>();

		_wrapper->VsisNative(_vsisNative);
		_wrapper->CreateGraphicsDevices();
		_vsisNative->RegisterForUpdatesNeeded(winrt::make_self<Scintilla::Internal::CallTipCallback>(_wrapper, _scintilla).get());

		// The SurfaceImageSource object's underlying
		// ISurfaceImageSourceNativeWithD2D object will contain the completed bitmap.

		if (const auto imageTarget{ Content().try_as<Border>() })
		{
			_imageTargetSizeChangedRevoker = imageTarget.SizeChanged(auto_revoke, { this, &CallTipControl::ImageTarget_SizeChanged });

			//_wrapper->SetMouseCaptureElement(imageTarget);

			const ImageBrush brush{};
			brush.ImageSource(virtualSurfaceImageSource);
			imageTarget.Background(brush);
		}

		// Todo: Evaluate if this is an appropriate place to add this event (and other code in this method)
		_suspendingRevoker = Application::Current().Suspending(auto_revoke, { this, &CallTipControl::Application_Suspending });
	}

	void CallTipControl::OnTapped(DUX::Input::TappedRoutedEventArgs const &e)
	{
		if (const auto originalSource{ e.OriginalSource().try_as<winrt::DUX::FrameworkElement>() };
			originalSource && originalSource.Name() == L"ImageTarget")
		{
			const auto pt{ e.GetPosition(originalSource) };
			_scintilla->HandleCallTipClick({ pt.X * _dpiScale, pt.Y * _dpiScale });
		}
		else
		{
			_scintilla->HandleCallTipClick({ -1, -1 });
		}
	}

	void CallTipControl::OnPointerMoved(DUX::Input::PointerRoutedEventArgs const &e)
	{
		__super::OnPointerPressed(e);

		if (const auto originalSource{ e.OriginalSource().try_as<winrt::DUX::FrameworkElement>() };
			originalSource && originalSource.Name() == L"ImageTarget")
		{
			const auto pt{ e.GetCurrentPoint(originalSource).Position() };
			_scintilla->HandleCallTipPointerMove({ pt.X * _dpiScale, pt.Y * _dpiScale });
		}
		else
		{
			_scintilla->HandleCallTipPointerMove({ -1, -1 });
		}
	}

	void CallTipControl::OnPointerPressed(DUX::Input::PointerRoutedEventArgs const &e)
	{
		__super::OnPointerPressed(e);

		Focus(FocusState::Pointer);
	}

	void CallTipControl::OnPointerReleased(DUX::Input::PointerRoutedEventArgs const &e)
	{
		__super::OnPointerReleased(e);

		e.Handled(true); // Prevents control from losing focus on pointer released
	}

	AutomationPeer CallTipControl::OnCreateAutomationPeer()
	{
		//return make<CallTipControlAutomationPeer>(*this);
		return __super::OnCreateAutomationPeer(); // Todo: implement automation peer
	}

	std::shared_ptr<::WinUIEditor::CallTipWrapper> CallTipControl::Wrapper()
	{
		return _wrapper;
	}

	void CallTipControl::ImageTarget_SizeChanged(IInspectable const &sender, SizeChangedEventArgs const &args)
	{
		if (_vsisNative)
		{
			auto width{ ConvertFromDipToPixelUnit(args.NewSize().Width, _dpiScale) };
			auto height{ ConvertFromDipToPixelUnit(args.NewSize().Height, _dpiScale) };
			_wrapper->Width(width);
			_wrapper->Height(height);
			_vsisNative->Resize(width, height);
		}
	}

	void CallTipControl::Application_Suspending(IInspectable const &sender, SuspendingEventArgs const &args)
	{
		// Required or crashes on resume
		// https://learn.microsoft.com/en-us/windows/uwp/gaming/directx-and-xaml-interop
		// https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_3/nf-dxgi1_3-idxgidevice3-trim
		// Todo: Should ClearResources get called too? https://github.com/microsoft/Win2D/blob/master/winrt/lib/drawing/CanvasDevice.cpp#L1040
		_wrapper->TrimDxgiDevice();
	}
}
