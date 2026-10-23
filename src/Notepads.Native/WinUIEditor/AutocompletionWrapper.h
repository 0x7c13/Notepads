// Copyright 2026 by Breece Walker
// See src/Notepads.Native/LICENSE for the original WinUIEdit license.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
// See LICENSE.txt in the project root for the Notepads modifications.

#pragma once

#include "Wrapper.h"

namespace WinUIEditor
{
	class AutocompletionWrapper : public Wrapper
	{
	public:
		AutocompletionWrapper(winrt::DUXC::Control const &control);

		void Show(bool visible) override;
		void Destroy() override;
		void SetPositionRelative(Scintilla::Internal::PRectangle rc, Wrapper const &wrapper) override;
		static std::shared_ptr<Wrapper> Create(winrt::DUXC::Primitives::Popup const &popup, float logicalDpi);

		void Append(winrt::hstring const &text);
		void Clear();
		int SelectedIndex();
		void SelectedIndex(int value);

		void SetListBoxDelegate(Scintilla::Internal::IListBoxDelegate *delegate);
		void NotifyDoubleClick();
		void NotifySelectionChange();

	private:
		winrt::weak_ref<winrt::DUXC::Primitives::Popup> _popup{ nullptr };
		Scintilla::Internal::IListBoxDelegate *_delegate{ nullptr };
		bool _hasFcu{ winrt::Windows::Foundation::Metadata::ApiInformation::IsApiContractPresent(L"Windows.Foundation.UniversalApiContract", 5) }; // Todo: Make static
	};
}
