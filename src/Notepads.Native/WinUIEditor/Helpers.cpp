// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "Helpers.h"

namespace WinUIEditor
{
	// Derived from https://github.com/microsoft/Windows-universal-samples/blob/main/Samples/ComplexInk/cpp/Common/DeviceResources.h
	int ConvertFromDipToPixelUnit(float val, float dpiAdjustmentRatio, bool rounded)
	{
		float pixelVal = val * dpiAdjustmentRatio;
		return static_cast<int>(rounded ? floorf(pixelVal + 0.5f) : pixelVal); // Todo: Test if this is ever necessary
	}

	typedef LONG(WINAPI *AppPolicyGetWindowingModelPtr)(HANDLE processToken, AppPolicyWindowingModel *policy);
	static AppPolicyGetWindowingModelPtr s_appPolicyGetWindowingModel;

	bool IsClassicWindow()
	{
		// To avoid issues with AppPolicyGetWindowingModel and Store certification, this method cannot be called normally
		// See https://github.com/microsoft/react-native-windows/pull/5369 (though OneCoreUAP_apiset.lib did not pass either)

		if (!s_appPolicyGetWindowingModel)
		{
			s_appPolicyGetWindowingModel =
				reinterpret_cast<AppPolicyGetWindowingModelPtr>(GetProcAddress(GetModuleHandleW(L"Api-ms-win-appmodel-runtime-l1-1-2.dll"), "AppPolicyGetWindowingModel"));
		}
		AppPolicyWindowingModel model;
		return !(s_appPolicyGetWindowingModel && SUCCEEDED(s_appPolicyGetWindowingModel(GetCurrentThreadEffectiveToken(), &model)))
			|| model != AppPolicyWindowingModel_Universal;
	}

	winrt::Windows::System::VirtualKeyModifiers GetKeyModifiersForCurrentThread()
	{
		auto modifiers{ winrt::Windows::System::VirtualKeyModifiers::None };

		const auto window{ winrt::Windows::UI::Core::CoreWindow::GetForCurrentThread() }; // Todo: is it worth it to store this?
		// Todo: Do we need to check Locked?
		// Todo: Left vs right keys?
		if ((window.GetKeyState(winrt::Windows::System::VirtualKey::Shift) & winrt::Windows::UI::Core::CoreVirtualKeyStates::Down) == winrt::Windows::UI::Core::CoreVirtualKeyStates::Down)
		{
			modifiers |= winrt::Windows::System::VirtualKeyModifiers::Shift;
		}
		if ((window.GetKeyState(winrt::Windows::System::VirtualKey::Control) & winrt::Windows::UI::Core::CoreVirtualKeyStates::Down) == winrt::Windows::UI::Core::CoreVirtualKeyStates::Down)
		{
			modifiers |= winrt::Windows::System::VirtualKeyModifiers::Control;
		}
		if ((window.GetKeyState(winrt::Windows::System::VirtualKey::Menu) & winrt::Windows::UI::Core::CoreVirtualKeyStates::Down) == winrt::Windows::UI::Core::CoreVirtualKeyStates::Down)
		{
			modifiers |= winrt::Windows::System::VirtualKeyModifiers::Menu; // Todo: KeyStatus.IsMenuKeyDown?
		}
		if ((window.GetKeyState(winrt::Windows::System::VirtualKey::LeftWindows) & winrt::Windows::UI::Core::CoreVirtualKeyStates::Down) == winrt::Windows::UI::Core::CoreVirtualKeyStates::Down
			|| (window.GetKeyState(winrt::Windows::System::VirtualKey::RightWindows) & winrt::Windows::UI::Core::CoreVirtualKeyStates::Down) == winrt::Windows::UI::Core::CoreVirtualKeyStates::Down)
		{
			modifiers |= winrt::Windows::System::VirtualKeyModifiers::Windows;
		}

		return modifiers;
	}
}
