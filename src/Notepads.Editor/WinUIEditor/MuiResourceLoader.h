#pragma once

namespace WinUIEditor
{
	struct MuiResourceLoader
	{
		MuiResourceLoader(winrt::Windows::Foundation::IInspectable const &modulePointer);
		~MuiResourceLoader();
		std::wstring_view GetString(WORD stringId, std::wstring_view fallback) const;

	private:
		HINSTANCE _moduleResources{ nullptr };
	};
}
