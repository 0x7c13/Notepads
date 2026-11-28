#include "pch.h"
#include "MuiResourceLoader.h"

// Normally defined in muiload.h
extern "C"
{
	HINSTANCE __stdcall LoadMUILibraryW(
		_In_ PCWSTR pszFullModuleName,
		_In_ DWORD dwLangConvention,
		_In_ LANGID LangID
	);

	BOOL __stdcall FreeMUILibrary(
		_In_ HMODULE hResModule
	);
}

// Defined to find a pointer inside the DLL with resources for lookup
interface Vtbl
{
	void *lpVtbl;
};

// muilib has a legacy reference to vsnwprintf
int (WINAPIV *__vsnwprintf)(wchar_t *, size_t, const wchar_t *, va_list) = _vsnwprintf;

namespace WinUIEditor
{
	MuiResourceLoader::MuiResourceLoader(winrt::Windows::Foundation::IInspectable const &modulePointer)
	{
		const auto modPtr = static_cast<Vtbl *>(winrt::get_abi(modulePointer))->lpVtbl;

		HMODULE module;
		WCHAR modulePath[MAX_PATH];
		if (GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
			static_cast<LPCWSTR>(modPtr),
			&module) &&
			GetModuleFileNameW(module, modulePath, MAX_PATH))
		{
			_moduleResources = LoadMUILibraryW(modulePath, MUI_LANGUAGE_NAME, 0);
		}
	}

	MuiResourceLoader::~MuiResourceLoader()
	{
		FreeMUILibrary(_moduleResources);
	}

	std::wstring_view MuiResourceLoader::GetString(WORD stringId, std::wstring_view fallback) const
	{
		if (_moduleResources)
		{
			// Strings are stored in blocks of 16

			constexpr int blockSize{ 16 };
			const auto block{ MAKEINTRESOURCEW(stringId / blockSize + 1) };

			auto resource{ FindResourceExW(_moduleResources, RT_STRING, block, MAKELANGID(LANG_NEUTRAL, SUBLANG_NEUTRAL)) };

			if (!resource)
			{
				// en-US fallback
				resource = FindResourceExW(_moduleResources, RT_STRING, block, MAKELANGID(LANG_ENGLISH, SUBLANG_ENGLISH_US));
			}

			if (resource)
			{
				if (const auto resourceHandle{ LoadResource(_moduleResources, resource) })
				{
					if (auto *resourcePtr{ static_cast<WCHAR *>(LockResource(resourceHandle)) })
					{
						// Data in the block is formatted 6length4then5chars...

						auto offset{ stringId & (blockSize - 1) }; // stringId % 16
						while (offset-- > 0)
						{
							resourcePtr += resourcePtr[0] + 1;
						}
						return std::wstring_view{ resourcePtr + 1, resourcePtr[0] };
					}
				}
			}
		}

		return fallback;
	}
}
