// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "MainWrapper.h"

namespace WinUIEditor
{
	MainWrapper::MainWrapper(winrt::DUXC::Control const &control) : Wrapper(control)
	{
	}

	void MainWrapper::SetMouseCapture(bool on)
	{
		if (!_mouseCaptureElement)
		{
			return;
		}

		if (on)
		{
			if (_lastPointer) // Todo: Check if works
			{
				_captured = _mouseCaptureElement.CapturePointer(_lastPointer);
			}
		}
		else
		{
			_mouseCaptureElement.ReleasePointerCaptures(); // Todo: Or just one?
			_captured = false;
		}
	}

	bool MainWrapper::HaveMouseCapture()
	{
		return _captured;
	}

	void MainWrapper::SetMouseCaptureElement(winrt::DUX::UIElement const &element)
	{
		_mouseCaptureElement = element;
		// Todo: Do we need to revoke?
		// Todo: Do we need a strong/weak reference?
		_mouseCaptureElement.PointerPressed([&](winrt::Windows::Foundation::IInspectable const &sender, winrt::DUX::Input::PointerRoutedEventArgs const &args)
			{
				_lastPointer = args.Pointer();
			});
		_mouseCaptureElement.PointerCaptureLost([&](winrt::Windows::Foundation::IInspectable const &sender, winrt::DUX::Input::PointerRoutedEventArgs const &args)
			{
				_captured = false;
			});
	}

	void MainWrapper::SetCursor(winrt::DCUR cursor)
	{
		winrt::Windows::UI::Core::CoreWindow::GetForCurrentThread().PointerCursor(winrt::Windows::UI::Core::CoreCursor{ cursor, 0 });
	}

	void MainWrapper::SetScrollBars(winrt::DUX::Controls::Primitives::ScrollBar const &horizontalScrollBar, winrt::DUX::Controls::Primitives::ScrollBar const &verticalScrollBar)
	{
		_horizontalScrollBar = horizontalScrollBar;
		_verticalScrollBar = verticalScrollBar;
	}

	bool MainWrapper::HasScrollBars()
	{
		return _horizontalScrollBar && _verticalScrollBar;
	}

	void MainWrapper::HorizontalScrollBarInset(int pixels)
	{
		if (!_horizontalScrollBar)
			return;
		const double inset{ pixels * 96.0 / LogicalDpi() };
		const winrt::DUX::Thickness margin{ inset, 0, 0, 0 };
		if (_horizontalScrollBar.Margin() != margin)
			_horizontalScrollBar.Margin(margin);
	}

	void MainWrapper::HorizontalScrollBarValue(double value)
	{
		_horizontalScrollBar.Value(value);
	}

	void MainWrapper::VerticalScrollBarValue(double value)
	{
		_verticalScrollBar.Value(value);
	}

	void MainWrapper::HorizontalScrollBarMinimum(double value)
	{
		_horizontalScrollBar.Minimum(value);
	}

	void MainWrapper::VerticalScrollBarMinimum(double value)
	{
		_verticalScrollBar.Minimum(value);
	}

	void MainWrapper::HorizontalScrollBarMaximum(double value)
	{
		_horizontalScrollBar.Maximum(value);
	}

	void MainWrapper::VerticalScrollBarMaximum(double value)
	{
		_verticalScrollBar.Maximum(value);
	}

	void MainWrapper::HorizontalScrollBarViewportSize(double value)
	{
		_horizontalScrollBar.ViewportSize(value);
	}

	void MainWrapper::VerticalScrollBarViewportSize(double value)
	{
		_verticalScrollBar.ViewportSize(value);
	}

	void MainWrapper::HorizontalScrollBarVisible(bool value)
	{
		_horizontalScrollBar.Visibility(value ? winrt::DUX::Visibility::Visible : winrt::DUX::Visibility::Collapsed);
	}

	void MainWrapper::VerticalScrollBarVisible(bool value)
	{
		_verticalScrollBar.Visibility(value ? winrt::DUX::Visibility::Visible : winrt::DUX::Visibility::Collapsed);
	}

	double MainWrapper::HorizontalScrollBarValue()
	{
		return _horizontalScrollBar.Value();
	}

	double MainWrapper::VerticalScrollBarValue()
	{
		return _verticalScrollBar.Value();
	}

	double MainWrapper::HorizontalScrollBarMinimum()
	{
		return _horizontalScrollBar.Minimum();
	}

	double MainWrapper::VerticalScrollBarMinimum()
	{
		return _verticalScrollBar.Minimum();
	}

	double MainWrapper::HorizontalScrollBarMaximum()
	{
		return _horizontalScrollBar.Maximum();
	}

	double MainWrapper::VerticalScrollBarMaximum()
	{
		return _verticalScrollBar.Maximum();
	}

	double MainWrapper::HorizontalScrollBarViewportSize()
	{
		return _horizontalScrollBar.ViewportSize();
	}

	double MainWrapper::VerticalScrollBarViewportSize()
	{
		return _verticalScrollBar.ViewportSize();
	}

	winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation> MainWrapper::StartDragAsync(winrt::DUI::PointerPoint const &pointerPoint)
	{
		return _mouseCaptureElement.StartDragAsync(pointerPoint);
	}

	void MainWrapper::Show(bool visible)
	{
	}

	void MainWrapper::Destroy()
	{
	}

	void MainWrapper::SetPositionRelative(Scintilla::Internal::PRectangle rc, Wrapper const &wrapper)
	{
	}
}
