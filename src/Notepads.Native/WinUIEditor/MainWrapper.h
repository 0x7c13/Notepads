// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once

#include "Wrapper.h"

namespace WinUIEditor
{
	class MainWrapper : public Wrapper
	{
	public:
		MainWrapper(winrt::DUXC::Control const &control);

		// Todo: Make abstract
		void SetMouseCapture(bool on);
		bool HaveMouseCapture();

		// Todo: Move to separate XAML class
		void SetMouseCaptureElement(winrt::DUX::UIElement const &element);
		void SetCursor(winrt::DCUR cursor);

		void SetScrollBars(winrt::DUX::Controls::Primitives::ScrollBar const &horizontalScrollBar, winrt::DUX::Controls::Primitives::ScrollBar const &verticalScrollBar);
		bool HasScrollBars();
		void HorizontalScrollBarInset(int pixels);
		void HorizontalScrollBarValue(double value);
		void VerticalScrollBarValue(double value);
		void HorizontalScrollBarMinimum(double value);
		void VerticalScrollBarMinimum(double value);
		void HorizontalScrollBarMaximum(double value);
		void VerticalScrollBarMaximum(double value);
		void HorizontalScrollBarViewportSize(double value);
		void VerticalScrollBarViewportSize(double value);
		void HorizontalScrollBarVisible(bool value);
		void VerticalScrollBarVisible(bool value);
		double HorizontalScrollBarValue();
		double VerticalScrollBarValue();
		double HorizontalScrollBarMinimum();
		double VerticalScrollBarMinimum();
		double HorizontalScrollBarMaximum();
		double VerticalScrollBarMaximum();
		double HorizontalScrollBarViewportSize();
		double VerticalScrollBarViewportSize();

		winrt::Windows::Foundation::IAsyncOperation<winrt::Windows::ApplicationModel::DataTransfer::DataPackageOperation> StartDragAsync(winrt::DUI::PointerPoint const &pointerPoint);

		void Show(bool visible) override;
		void Destroy() override;
		void SetPositionRelative(Scintilla::Internal::PRectangle rc, Wrapper const &wrapper) override;

	private:
		winrt::DUX::Input::Pointer _lastPointer{ nullptr };
		winrt::DUX::UIElement _mouseCaptureElement{ nullptr };
		winrt::DUX::Controls::Primitives::ScrollBar _horizontalScrollBar{ nullptr };
		winrt::DUX::Controls::Primitives::ScrollBar _verticalScrollBar{ nullptr };
		bool _captured{ false };
	};
}
