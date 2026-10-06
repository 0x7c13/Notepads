// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#include "pch.h"
#include "EditorBaseControl.h"
#include "AutocompletionControl.h"
#include "AutocompletionWrapper.h"

namespace WinUIEditor
{
	AutocompletionWrapper::AutocompletionWrapper(winrt::DUXC::Control const &control) : Wrapper(control)
	{
	}

	void AutocompletionWrapper::Show(bool visible)
	{
		if (const auto popup{ _popup.get() })
		{
			popup.Tag(winrt::box_value(visible));
			if (_hasFcu)
			{
				const auto dispatcherQueue{
					winrt::DUD::DispatcherQueue::GetForCurrentThread()
				};
				dispatcherQueue.TryEnqueue([=]()
					{
						popup.IsOpen(winrt::unbox_value<bool>(popup.Tag()));
					});
			}
			else
			{
				popup.Dispatcher().TryRunAsync(winrt::Windows::UI::Core::CoreDispatcherPriority::Normal, [=]()
					{
						popup.IsOpen(winrt::unbox_value<bool>(popup.Tag()));
					});
			}
		}
	}

	void AutocompletionWrapper::Destroy()
	{
		Show(false);
		Clear();
		SelectedIndex(-1);
	}

	void AutocompletionWrapper::SetPositionRelative(Scintilla::Internal::PRectangle rc, Wrapper const &wrapper)
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

		if (const auto control{ _control.get().try_as<winrt::WinUIEditor::implementation::AutocompletionControl>() })
		{
			control->Width(width);
			control->Height(height);
		}
		if (const auto popup{ _popup.get() })
		{
			popup.HorizontalOffset(x);
			popup.VerticalOffset(y);
		}
	}

	std::shared_ptr<Wrapper> AutocompletionWrapper::Create(winrt::DUXC::Primitives::Popup const &popup, float logicalDpi)
	{
		auto autocompletion{ popup.Child().try_as<winrt::WinUIEditor::implementation::AutocompletionControl>() };
		if (!autocompletion)
		{
			autocompletion = winrt::make_self<winrt::WinUIEditor::implementation::AutocompletionControl>();
			autocompletion->MaxWidth(std::numeric_limits<double>::infinity());
			autocompletion->Padding(winrt::DUX::Thickness{ 0, 0, 0, 0 });
			autocompletion->Wrapper()->LogicalDpi(logicalDpi);
			autocompletion->IsTabStop(false);
			autocompletion->HorizontalAlignment(winrt::DUX::HorizontalAlignment::Center);
			autocompletion->VerticalAlignment(winrt::DUX::VerticalAlignment::Center);
			autocompletion->Wrapper()->_popup = popup;
			popup.Child(*autocompletion);
		}
		return autocompletion->Wrapper();
	}

	void AutocompletionWrapper::Append(winrt::hstring const &text)
	{
		if (const auto control{ _control.get().try_as<winrt::WinUIEditor::implementation::AutocompletionControl>() })
		{
			const auto list{ control->ItemsSource().as<winrt::Windows::Foundation::Collections::IObservableVector<winrt::hstring>>() };
			list.Append(text);
		}
	}

	void AutocompletionWrapper::Clear()
	{
		if (const auto control{ _control.get().try_as<winrt::WinUIEditor::implementation::AutocompletionControl>() })
		{
			const auto list{ control->ItemsSource().as<winrt::Windows::Foundation::Collections::IObservableVector<winrt::hstring>>() };
			list.Clear();
		}
	}

	int AutocompletionWrapper::SelectedIndex()
	{
		const auto control{ _control.get().try_as<winrt::WinUIEditor::implementation::AutocompletionControl>() };
		return control ? control->SelectedIndex() : -1;
	}

	void AutocompletionWrapper::SelectedIndex(int value)
	{
		if (const auto control{ _control.get().try_as<winrt::WinUIEditor::implementation::AutocompletionControl>() })
		{
			control->SelectedIndex(value);
		}
	}

	void AutocompletionWrapper::SetListBoxDelegate(Scintilla::Internal::IListBoxDelegate *delegate)
	{
		_delegate = delegate;
	}

	void AutocompletionWrapper::NotifyDoubleClick()
	{
		if (_delegate)
		{
			Scintilla::Internal::ListBoxEvent event{ Scintilla::Internal::ListBoxEvent::EventType::doubleClick };
			_delegate->ListNotify(&event);
		}
	}
	void AutocompletionWrapper::NotifySelectionChange()
	{
		if (_delegate)
		{
			Scintilla::Internal::ListBoxEvent event{ Scintilla::Internal::ListBoxEvent::EventType::selectionChange };
			_delegate->ListNotify(&event);
		}
	}
}
