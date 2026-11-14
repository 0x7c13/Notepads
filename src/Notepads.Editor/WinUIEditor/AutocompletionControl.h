#pragma once

#include "AutocompletionControl.g.h"

#include "AutocompletionWrapper.h"

namespace winrt::WinUIEditor::implementation
{
	struct AutocompletionControl : AutocompletionControlT<AutocompletionControl>
	{
		AutocompletionControl();

		void OnApplyTemplate();
		void OnDoubleTapped(DUX::Input::DoubleTappedRoutedEventArgs const &args);

		void OnSelectionChanged(IInspectable const &sender, DUXC::SelectionChangedEventArgs const &e);

		static DUX::DependencyProperty ItemsSourceProperty();
		IInspectable ItemsSource();

		std::shared_ptr<::WinUIEditor::AutocompletionWrapper> Wrapper();

		int SelectedIndex();
		void SelectedIndex(int value);

	private:
		std::shared_ptr<::WinUIEditor::AutocompletionWrapper> _wrapper{ nullptr };
		static DUX::DependencyProperty s_itemsSourceProperty;
		int _selectedIndex{ -1 };
		DUXC::Primitives::Selector::SelectionChanged_revoker _selectionChangedRevoker{};
	};
}

namespace winrt::WinUIEditor::factory_implementation
{
	struct AutocompletionControl : AutocompletionControlT<AutocompletionControl, implementation::AutocompletionControl>
	{
	};
}
