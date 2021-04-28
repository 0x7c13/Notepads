#pragma once
#pragma comment(lib, "shell32")
#define STRICT
#define STRICT_TYPED_ITEMIDS
#include "windows.h"
#include "shellapi.h"
#include "shlobj_core.h"
#include "winrt/base.h"

#ifdef _PRODUCTION
#define AUMID                                    L"19282JackieLiu.Notepads-Beta_echhpq9pdbte8!App"
#else
#define AUMID                                    L"Notepads-Dev_echhpq9pdbte8!App"
#endif

#define OPEN                                     L"open"
#define PRINT                                    L"print"

#define NOTEPAD_PRINT_COMMAND                    L"/p"
#define NOTEPAD_OPEN_WITH_ANSI_ENCODING_COMMAND  L"/a"
#define NOTEPAD_OPEN_WITH_UTF16_ENCODING_COMMAND L"/w"

namespace winrt
{
    template <typename T, typename F, typename...Args>
    impl::com_ref<T> parse(F function, Args&& ...args)
    {
        T* result{};
        check_hresult(function(args..., &result));
        return { result, take_ownership_from_abi };
    }
}

using namespace winrt;

#ifndef _DEBUG
INT APIENTRY wWinMain(
    _In_     HINSTANCE /* hInstance */,
    _In_opt_ HINSTANCE /* hPrevInstance */,
    _In_     LPWSTR    /* lpCmdLine */,
    _In_     INT       /* nCmdShow */)
#else
INT main()
#endif
{
    init_apartment();

    auto nArgs = 0;
    auto args = CommandLineToArgvW(GetCommandLine(), &nArgs);
    if (nArgs > 1)
    {
        auto nIgnore = 1;
        auto verb = OPEN;
        if (wcscmp(NOTEPAD_PRINT_COMMAND, args[1]) == 0)
        {
            nIgnore = 2;
            verb = PRINT;
        }

        auto rgpidl = com_array<PIDLIST_ABSOLUTE>(nArgs - nIgnore);
        for (auto ppidl = rgpidl.begin(); ppidl < rgpidl.end(); ppidl++, nArgs--)
        {
            SHParseDisplayName(args[nArgs - 1], NULL, ppidl, 0, NULL);
        }

        if (rgpidl.size() > 0)
        {
            auto pid = DWORD(0);
            auto ppsia = parse<IShellItemArray>(SHCreateShellItemArrayFromIDLists, rgpidl.size(), rgpidl.data());
            auto appActivationMgr = create_instance<IApplicationActivationManager>(CLSID_ApplicationActivationManager);
            appActivationMgr->ActivateForFile(AUMID, ppsia.get(), verb, &pid);
#ifdef _DEBUG
            printf("Launched files with process id: %d", pid);
            Sleep(2000);
#endif
        }
    }

    LocalFree(args);
    uninit_apartment();
}