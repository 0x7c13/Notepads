#include "pch.h"
#include "appcenter.h"

using namespace std;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::ApplicationModel::AppService;
using namespace Windows::Foundation;
using namespace Windows::Foundation::Collections;

HANDLE appExitJob = NULL;

AppServiceConnection interopServiceConnection = NULL;

fire_and_forget launchElevatedProcess()
{
    TCHAR fileName[MAX_PATH];
    GetModuleFileName(NULL, fileName, MAX_PATH);

    SHELLEXECUTEINFO shExInfo = { 0 };
    shExInfo.cbSize = sizeof(shExInfo);
    shExInfo.fMask = SEE_MASK_NOCLOSEPROCESS;
    shExInfo.hwnd = 0;
    shExInfo.lpVerb = L"runas";
    shExInfo.lpFile = fileName;
    shExInfo.lpParameters = L"";
    shExInfo.lpDirectory = 0;
    shExInfo.nShow = SW_SHOW;
    shExInfo.hInstApp = 0;

    auto message = ValueSet();
    vector<pair<const CHAR*, string>> properties;
    message.Insert(InteropCommandLabel, box_value(CreateElevetedExtensionCommandStr));
    if (ShellExecuteEx(&shExInfo))
    {
        // Create Job to close child process when parent exits/crashes.
        if (appExitJob) CloseHandle(appExitJob);
        appExitJob = CreateJobObject(NULL, NULL);
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION info = { 0 };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(appExitJob, JobObjectExtendedLimitInformation, &info, sizeof(info));
        AssignProcessToJobObject(appExitJob, shExInfo.hProcess);

        message.Insert(InteropCommandAdminCreatedLabel, box_value(true));
        printDebugMessage(L"Adminstrator Extension has been launched.");
        properties.push_back(pair("Accepted", "True"));
    }
    else
    {
        message.Insert(InteropCommandAdminCreatedLabel, box_value(false));
        printDebugMessage(L"User canceled launching of Adminstrator Extension.");
        properties.push_back(pair("Denied", "True"));
    }
    co_await interopServiceConnection.SendMessageAsync(message);
    AppCenter::trackEvent("OnAdminstratorPrivilageGranted", properties);
}

void onConnectionServiceRequestRecieved(AppServiceConnection sender, AppServiceRequestReceivedEventArgs args)
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
    else if (command == ExitAppCommandStr)
    {
        exitApp();
    }

    messageDeferral.Complete();
}

void onConnectionServiceClosed(AppServiceConnection sender, AppServiceClosedEventArgs args)
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
        exitApp();
    }

    auto message = ValueSet();
    message.Insert(InteropCommandLabel, box_value(RegisterExtensionCommandStr));
    co_await interopServiceConnection.SendMessageAsync(message);

    printDebugMessage(L"Successfully created App Service.");
}