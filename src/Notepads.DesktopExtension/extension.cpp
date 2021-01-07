#include "pch.h"
#include "appcenter.h"

using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::ApplicationModel::AppService;
using namespace Windows::Foundation;
using namespace Windows::Foundation::Collections;

AppServiceConnection interopServiceConnection = NULL;
HANDLE appExitJob = NULL;

fire_and_forget launchElevatedProcess()
{
    TCHAR fileName[MAX_PATH];
    GetModuleFileName(NULL, fileName, MAX_PATH);

    SHELLEXECUTEINFO shExInfo
    { 
        .cbSize = sizeof(shExInfo),
        .fMask = SEE_MASK_NOCLOSEPROCESS,
        .hwnd = 0,
        .lpVerb = L"runas",
        .lpFile = fileName,
        .lpParameters = L"",
        .lpDirectory = 0,
        .nShow = SW_SHOW,
        .hInstApp = 0
    };

    auto message = ValueSet();
    vector<pair<const CHAR*, string>> properties;
    message.Insert(InteropCommandLabel, box_value(CreateElevetedExtensionCommandStr));
    if (ShellExecuteEx(&shExInfo))
    {
        // Create Job to close child process when parent exits/crashes.
        TerminateJobObject(appExitJob, 0);
        if (appExitJob) CloseHandle(appExitJob);
        appExitJob = CreateJobObject(NULL, NULL);

        JOBOBJECT_EXTENDED_LIMIT_INFORMATION info {
            .BasicLimitInformation {
                .LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE 
            }
        };
        SetInformationJobObject(appExitJob, JobObjectExtendedLimitInformation, &info, sizeof(info));
        if (shExInfo.hProcess) AssignProcessToJobObject(appExitJob, shExInfo.hProcess);

        message.Insert(InteropCommandAdminCreatedLabel, box_value(true));
        printDebugMessage(L"Adminstrator Extension has been launched.");
        properties.push_back(pair("Accepted", "True"));
    }
    else
    {
        message.Insert(InteropCommandAdminCreatedLabel, box_value(false));
        printDebugMessage(L"Launching of Adminstrator Extension was cancelled.");

        pair<DWORD, wstring> ex = getLastErrorDetails();
        properties.insert(properties.end(), 
            {
                pair("Denied", "True"),
                pair("Error Code", to_string(ex.first)),
                pair("Error Message", to_string(ex.second))
            });
    }
    co_await interopServiceConnection.SendMessageAsync(message);
    AppCenter::trackEvent("OnAdminstratorPrivilageRequested", properties);
}

VOID onConnectionServiceRequestRecieved(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
{
    // Get a deferral because we use an awaitable API below to respond to the message
    // and we don't want this call to get canceled while we are waiting.
    auto messageDeferral = args.GetDeferral();
    setExceptionHandling();

    auto message = args.Request().Message();
    if (!message.HasKey(InteropCommandLabel)) return;

    auto command = unbox_value_or<hstring>(message.TryLookup(InteropCommandLabel), L"");
    if (command == CreateElevetedExtensionCommandStr)
    {
        launchElevatedProcess();
    }

    messageDeferral.Complete();
}

VOID onConnectionServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
{
    exitApp();
}

fire_and_forget initializeInteropService()
{
    printDebugMessage(L"Successfully started Desktop Extension.");

    interopServiceConnection = AppServiceConnection();
    interopServiceConnection.AppServiceName(InteropServiceName);
    interopServiceConnection.PackageFamilyName(Package::Current().Id().FamilyName());

    interopServiceConnection.RequestReceived(onConnectionServiceRequestRecieved);
    interopServiceConnection.ServiceClosed(onConnectionServiceClosed);

    auto status = co_await interopServiceConnection.OpenAsync();
    if (status != AppServiceConnectionStatus::Success)
    {
        pair<DWORD, wstring> ex = getLastErrorDetails();
        vector<pair<const CHAR*, string>> properties
        {
            pair("Error Code", to_string(ex.first)),
            pair("Error Message", to_string(ex.second))
        };

        AppCenter::trackEvent("OnWin32ConnectionToAppServiceFailed", properties);
        exitApp();
    }

    auto message = ValueSet();
    message.Insert(InteropCommandLabel, box_value(RegisterExtensionCommandStr));
    co_await interopServiceConnection.SendMessageAsync(message);

    printDebugMessage(L"Successfully created App Service.");
}