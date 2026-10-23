// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "EditorBaseControl.h"
#include "CallTipControl.h"
#include "CallTipWrapper.h"

namespace WinUIEditor
{
	CallTipWrapper::CallTipWrapper(winrt::DUXC::Control const &control) : Wrapper(control)
	{
	}

	void CallTipWrapper::Show(bool visible)
	{
		if (const auto popup{ _popup.get() })
		{
			popup.Visibility(visible ? winrt::DUX::Visibility::Visible : winrt::DUX::Visibility::Collapsed);
		}
	}

	void CallTipWrapper::Destroy()
	{
		if (const auto popup{ _popup.get() })
		{
			popup.IsOpen(false);
		}
	}

	void CallTipWrapper::SetPositionRelative(Scintilla::Internal::PRectangle rc, Wrapper const &wrapper)
	{
		// Todo: Make sure to position such that the tooltip does not hang off the monitor
		// Transform to root rather than other window because that is how XAML Popup offset will be set
		const auto dpiScale{ LogicalDpi() / 96.f };
		float x = rc.left / dpiScale;
		float y = rc.top / dpiScale;
		float width = rc.Width() / dpiScale;
		float height = rc.Height() / dpiScale;
		if (!wrapper.TransformToRoot(x, y))
		{
			return;
		}

		if (const auto control{ _control.get().try_as<winrt::WinUIEditor::implementation::CallTipControl>() })
		{
			control->Width(width);
			control->Height(height);
			if (const auto content{ control->Content().try_as<winrt::DUX::FrameworkElement>() })
			{
				content.Width(width);
				content.Height(height - (LogicalDpi() > 120 ? 14 : 13)); // Hack to make spacing more pleasant on all screens
			}
		}
		if (const auto popup{ _popup.get() })
		{
			popup.HorizontalOffset(x);
			popup.VerticalOffset(y);
		}
	}

	std::shared_ptr<Wrapper> CallTipWrapper::Create(winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> const &scintilla, winrt::DUX::XamlRoot const &xamlRoot, float logicalDpi, winrt::DUX::ElementTheme theme)
	{
		static bool hasUac8{ winrt::Windows::Foundation::Metadata::ApiInformation::IsApiContractPresent(L"Windows.Foundation.UniversalApiContract", 8) }; // Todo: Make static

		const auto tooltip{ winrt::make_self<winrt::WinUIEditor::implementation::CallTipControl>(scintilla) };
		tooltip->MaxWidth(std::numeric_limits<double>::infinity());
		tooltip->Padding(winrt::DUX::Thickness{ 0, 0, 0, 0 });
		tooltip->Wrapper()->LogicalDpi(logicalDpi);
		tooltip->IsTabStop(false);
		tooltip->HorizontalAlignment(winrt::DUX::HorizontalAlignment::Center);
		tooltip->VerticalAlignment(winrt::DUX::VerticalAlignment::Center);
		winrt::DUXC::Primitives::Popup popup{};
		tooltip->Wrapper()->_popup = popup;
		if (hasUac8)
		{
			// Per WinUI 3 source code, the shadow is added to the tooltip ContentPresenter for animation reasons
			// We add it to the popup level instead because it is easier and we are not animating tooltips
			// Setting translation on the tooltip level causes rendering to break when the popup is in some positions
			popup.Shadow(winrt::DUX::Media::ThemeShadow{});
			popup.Translation(winrt::Windows::Foundation::Numerics::float3{ 0, 0, 16 });
			// Needed for AppWindow
			popup.ShouldConstrainToRootBounds(false);
			popup.XamlRoot(xamlRoot);
		}
		// Todo: Live theme changes
		popup.RequestedTheme(theme);
		popup.IsOpen(true);
		popup.Visibility(winrt::DUX::Visibility::Collapsed);
		popup.Child(*tooltip);
		return tooltip->Wrapper();
	}
}
