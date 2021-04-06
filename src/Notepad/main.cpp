#pragma once
#pragma comment(lib, "shell32")
#define STRICT
#define STRICT_TYPED_ITEMIDS
#include "iostream"
#include "windows.h"
#include "shellapi.h"
#include "shlobj_core.h"
#include "winrt/base.h"

#ifdef _PRODUCTION
#define AUMID                               L"19282JackieLiu.Notepads-Beta_echhpq9pdbte8!App"
#else
#define AUMID                               L"Notepads-Dev_echhpq9pdbte8!App"
#endif

#define OPEN                                L"open"
#define PRINT                               L"print"

#define NOTEPADPRINTCOMMAND                 L"/p"
#define NOTEPADOPENWITHANSIENCODINGCOMMAND  L"/a"
#define NOTEPADOPENWITHUTF16ENCODINGCOMMAND L"/w"

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

using namespace std;
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
    auto szArglist = CommandLineToArgvW(GetCommandLine(), &nArgs);
    if (szArglist && nArgs > 1)
    {
        auto verb = OPEN;
        if (wcscmp(NOTEPADPRINTCOMMAND, szArglist[1]) == 0) verb = PRINT;
        auto increment = wcscmp(OPEN, verb) == 0 ? 1 : 2;

        auto ct = nArgs - increment;
        auto rgpidl = new(std::nothrow) PIDLIST_ABSOLUTE[ct];
        if (rgpidl)
        {
            auto cpidl = 0;
            for (auto hr = S_OK; SUCCEEDED(hr) && cpidl < ct; cpidl++)
            {
                hr = SHParseDisplayName(szArglist[cpidl + increment], NULL, &rgpidl[cpidl], 0, NULL);
            }

            if (cpidl > 0)
            {
                auto pid = DWORD(0);
                auto ppsia = parse<IShellItemArray>(SHCreateShellItemArrayFromIDLists, cpidl, rgpidl);
                auto appActivationMgr = create_instance<IApplicationActivationManager>(CLSID_ApplicationActivationManager);
                appActivationMgr->ActivateForFile(AUMID, ppsia.get(), verb, &pid);
#ifdef _DEBUG
                cout << "Launched files with process id: " << pid;
                Sleep(2000);
#endif
            }

            for (auto i = 0; i < cpidl; i++)
            {
                CoTaskMemFree(rgpidl[i]);
            }
        }

        delete[] rgpidl;
    }

    LocalFree(szArglist);
    uninit_apartment();
}