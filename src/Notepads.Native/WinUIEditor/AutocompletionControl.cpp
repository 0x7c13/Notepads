#include "pch.h"
#include "AutocompletionControl.h"
#if __has_include("AutocompletionControl.g.cpp")
#include "AutocompletionControl.g.cpp"
#endif

using namespace winrt;
using namespace DUX;
using namespace DUXC;
using namespace DUXC::Primitives;
using namespace Windows::Foundation;

namespace winrt::WinUIEditor::implementation
{
	AutocompletionControl::AutocompletionControl()
	{
		DefaultStyleKey(winrt::box_value(L"WinUIEditor.AutocompletionControl"));

		SetValue(s_itemsSourceProperty, single_threaded_observable_vector<hstring>());

		_wrapper = std::make_shared<::WinUIEditor::AutocompletionWrapper>(*this);
	}

	void AutocompletionControl::OnApplyTemplate()
	{
		SelectedIndex(_selectedIndex);

		_selectionChangedRevoker.revoke();

		const auto suggestions{ GetTemplateChild(L"Suggestions") };
		if (const auto list{ suggestions.try_as<Selector>() })
		{
			_selectionChangedRevoker = list.SelectionChanged(auto_revoke, { this, &AutocompletionControl::OnSelectionChanged });
		}
	}

	void AutocompletionControl::OnDoubleTapped(DUX::Input::DoubleTappedRoutedEventArgs const &args)
	{
		if (args.OriginalSource().try_as<ListViewItemPresenter>() || args.OriginalSource().try_as<TextBlock>())
		{
			_wrapper->NotifyDoubleClick();
		}
	}

	void AutocompletionControl::OnSelectionChanged(IInspectable const &sender, DUXC::SelectionChangedEventArgs const &e)
	{
		if (e.AddedItems().Size() > 0)
		{
			_wrapper->NotifySelectionChange();
		}
	}

	DependencyProperty AutocompletionControl::ItemsSourceProperty()
	{
		return s_itemsSourceProperty;
	}

	IInspectable AutocompletionControl::ItemsSource()
	{
		return unbox_value<IInspectable>(GetValue(s_itemsSourceProperty));
	}

	std::shared_ptr<::WinUIEditor::AutocompletionWrapper> AutocompletionControl::Wrapper()
	{
		return _wrapper;
	}

	int AutocompletionControl::SelectedIndex()
	{
		if (const auto list{ GetTemplateChild(L"Suggestions").try_as<Selector>() })
		{
			return list.SelectedIndex();
		}
		else
		{
			return _selectedIndex;
		}
	}

	void AutocompletionControl::SelectedIndex(int value)
	{
		_selectedIndex = value;
		const auto suggestions{ GetTemplateChild(L"Suggestions") };
		if (const auto list{ suggestions.try_as<Selector>() })
		{
			list.SelectedIndex(value);
		}
		else
		{
			_wrapper->NotifySelectionChange();
		}
		if (const auto list{ suggestions.try_as<ListViewBase>() })
		{
			list.ScrollIntoView(list.SelectedItem());
		}
	}

	DependencyProperty AutocompletionControl::s_itemsSourceProperty{ DependencyProperty::Register(L"ItemsSource", xaml_typename<IInspectable>(), xaml_typename<WinUIEditor::AutocompletionControl>(), nullptr) };
}
