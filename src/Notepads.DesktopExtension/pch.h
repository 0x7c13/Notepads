#pragma once
#pragma comment(lib, "shell32")
#include "iostream"
#include "sstream"
#include "Windows.h"
#include "shellapi.h"
#include "winrt/Windows.ApplicationModel.h"
#include "winrt/Windows.ApplicationModel.AppService.h"
#include "winrt/Windows.Foundation.h"
#include "winrt/Windows.Foundation.Collections.h"
#include "winrt/Windows.Security.ExchangeActiveSyncProvisioning.h"
#include "winrt/Windows.Storage.h"
#include "winrt/Windows.System.h"
#include "winrt/Windows.System.Profile.h"
#include "curl/curl.h"

// These values depend upon constant fields described in ..\Notepads\Settings\SettingsKey.cs.
// Changing value in one place require changing variable with similar name in another.
/////////////////////////////////////////////////////////////////////////////////////////////
constexpr LPCTSTR AppCenterSecret = NULL;
constexpr LPCTSTR AppCenterInstallIdStr = L"AppCenterInstallIdStr";
constexpr LPCTSTR InteropServiceName = L"DesktopExtensionServiceConnection";
constexpr LPCTSTR PackageSidStr = L"PackageSidStr";
constexpr LPCTSTR AdminPipeConnectionNameStr = L"NotepadsAdminWritePipe";
constexpr LPCTSTR InteropCommandLabel = L"Command";
constexpr LPCTSTR InteropCommandAdminCreatedLabel = L"AdminCreated";
constexpr LPCTSTR RegisterExtensionCommandStr = L"RegisterExtension";
constexpr LPCTSTR CreateElevetedExtensionCommandStr = L"CreateElevetedExtension";
constexpr LPCTSTR ExitAppCommandStr = L"ExitApp";
/////////////////////////////////////////////////////////////////////////////////////////////

bool isElevatedProcess();
void setExceptionHandling();
void exitApp();

void printDebugMessage(LPCTSTR message, DWORD sleepTime = 0);
void printDebugMessage(LPCTSTR message, LPCTSTR parameter, DWORD sleepTime = 0);
std::string getTimeStamp();
std::string getTimeStamp(const char* format);
std::string to_string(winrt::Windows::System::ProcessorArchitecture arch);
std::string base64_encode(const std::string& in);

winrt::fire_and_forget initializeInteropService();
winrt::fire_and_forget launchElevatedProcess();

void initializeAdminService();
winrt::Windows::Foundation::IInspectable readSettingsKey(winrt::hstring key);

winrt::Windows::Foundation::IAsyncAction logLastError(LPCTSTR errorTitle);
winrt::fire_and_forget initializeLogging(LPCTSTR trailStr);

namespace AppCenter
{
	namespace
	{
		static std::string launchTimeStamp;
		static char* appCenterJSON;
		static struct curl_slist* headerList;
	}

	void start();
	void trackError(DWORD errorCode, std::string message, bool isFatal);
}