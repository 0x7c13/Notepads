#pragma once

#include "Wrapper.h"

namespace WinUIEditor
{
	class CallTipWrapper : public Wrapper
	{
	public:
		CallTipWrapper(winrt::DUXC::Control const &control);

		void Show(bool visible) override;
		void Destroy() override;
		void SetPositionRelative(Scintilla::Internal::PRectangle rc, Wrapper const &wrapper) override;
		static std::shared_ptr<Wrapper> Create(winrt::com_ptr<Scintilla::Internal::ScintillaWinUI> const &scintilla, winrt::DUX::XamlRoot const &xamlRoot, float logicalDpi, winrt::DUX::ElementTheme theme);

	private:
		winrt::weak_ref<winrt::DUXC::Primitives::Popup> _popup{ nullptr };
	};
}
