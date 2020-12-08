// Documentation is at https://docs.microsoft.com/en-us/appcenter/diagnostics/upload-crashes
#include "pch.h"
#include "resource.h"
#include "combaseapi.h"
#include "rapidjson/document.h"
#include "rapidjson/writer.h"
#include "rapidjson/stringbuffer.h"

#define APPCENTER_ENDPOINT "https://in.appcenter.ms/logs?Api-Version=1.0.0"

using namespace std;
using namespace rapidjson;
using namespace winrt;
using namespace Windows::ApplicationModel;
using namespace Windows::Foundation;
using namespace Windows::Security::ExchangeActiveSyncProvisioning;
using namespace Windows::Storage;
using namespace Windows::System;
using namespace Windows::System::Profile;

void AppCenter::start()
{
	if (!AppCenterSecret) return;

	launchTimeStamp = getTimeStamp();

	HMODULE handle = ::GetModuleHandle(NULL);
	HRSRC rc = FindResource(handle, MAKEINTRESOURCE(APPCENTER_JSON), MAKEINTRESOURCE(JSONFILE));
	HGLOBAL rcData = LoadResource(handle, rc);
	DWORD size = SizeofResource(handle, rc);
	appCenterJSON = static_cast<char*>(LockResource(rcData));

	headerList = curl_slist_append(headerList, "Content-Type: application/json");

	stringstream appSecretHeader;
	appSecretHeader << "app-secret: " << to_string(AppCenterSecret);
	headerList = curl_slist_append(headerList, appSecretHeader.str().c_str());

	hstring installId = unbox_value_or<hstring>(readSettingsKey(AppCenterInstallIdStr), L"");
	if (installId == L"") return;
	stringstream installIdHeader;
	installIdHeader << "install-id: " << to_string(installId);
	headerList = curl_slist_append(headerList, installIdHeader.str().c_str());
}

void AppCenter::trackError(DWORD errorCode, string message, bool isFatal)
{
	if (!AppCenterSecret) return;

	string timeStamp = getTimeStamp();

	Document errorReportForm;
	errorReportForm.Parse(appCenterJSON);

	string crashReportId = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	crashReportId.erase(0, crashReportId.find_first_not_of('{')).erase(crashReportId.find_last_not_of('}') + 1);
	string errorAttachmentId = to_string(to_hstring(GuidHelper::CreateNewGuid()));
	errorAttachmentId.erase(0, errorAttachmentId.find_first_not_of('{')).erase(errorAttachmentId.find_last_not_of('}') + 1);

	ULONGLONG version = stoull(AnalyticsInfo::VersionInfo().DeviceFamilyVersion().c_str());
	ULONGLONG major = (version & 0xFFFF000000000000L) >> 48;
	ULONGLONG minor = (version & 0x0000FFFF00000000L) >> 32;
	ULONGLONG build = (version & 0x00000000FFFF0000L) >> 16;
	ULONGLONG revision = (version & 0x000000000000FFFFL);
	stringstream osVersionBuilder;
	osVersionBuilder << major << "." << minor << "." << build;
	string osVersion = osVersionBuilder.str();
	osVersionBuilder << "." << revision;
	string osBuild = osVersionBuilder.str();

	EasClientDeviceInformation oemInfo = EasClientDeviceInformation();

	stringstream screenSize;
	RECT desktop;
	GetWindowRect(GetDesktopWindow(), &desktop);
	screenSize << desktop.right << "x" << desktop.bottom;

	WCHAR locale[LOCALE_NAME_MAX_LENGTH + 1];
	LCIDToLocaleName(GetThreadLocale(), locale, LOCALE_NAME_MAX_LENGTH, 0);
	WCHAR localeDisplayName[LOCALE_NAME_MAX_LENGTH + 1];
	GetLocaleInfoEx(locale, LOCALE_SENGLISHDISPLAYNAME, localeDisplayName, LOCALE_NAME_MAX_LENGTH);

	TIME_ZONE_INFORMATION timeZoneInfo;
	GetTimeZoneInformation(&timeZoneInfo);

	stringstream exceptionProp;
	exceptionProp << "Win32Exception: Exception of code no. " << errorCode << " was thrown.";

	stringstream errorAttachmentBuilder;
	errorAttachmentBuilder
		<< "Exception: Win32Exception code no. " << errorCode << ", "
		<< "Message: " << message << ", "
		<< "IsDesktopExtension: True" << ", "
		<< "IsElevated: " << (isElevatedProcess() ? "True" : "False");
	string errorAttachment = base64_encode(errorAttachmentBuilder.str());

	Value& logReports = errorReportForm["logs"];
	for (int i = 0; i < logReports.GetArray().Size(); i++)
	{
		logReports[i]["timestamp"].SetString(timeStamp.c_str(), errorReportForm.GetAllocator());

		logReports[i]["device"]["osName"].SetString(to_string(oemInfo.OperatingSystem()).c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["osVersion"].SetString(osVersion.c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["osBuild"].SetString(osBuild.c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["model"].SetString(to_string(oemInfo.SystemProductName()).c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["oemName"].SetString(to_string(oemInfo.SystemManufacturer()).c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["screenSize"].SetString(screenSize.str().c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["locale"].SetString(to_string(locale).c_str(), errorReportForm.GetAllocator());
		logReports[i]["device"]["timeZoneOffset"].SetInt(-1 * timeZoneInfo.Bias);

		if (i == 0)
		{
			logReports[i]["appLaunchTimestamp"].SetString(launchTimeStamp.c_str(), errorReportForm.GetAllocator());
			logReports[i]["processId"].SetString(to_string(GetCurrentProcessId()).c_str(), errorReportForm.GetAllocator());
			logReports[i]["id"].SetString(crashReportId.c_str(), errorReportForm.GetAllocator());
			logReports[i]["fatal"].SetBool(isFatal);
			logReports[i]["errorThreadId"].SetInt(GetCurrentThreadId());

			logReports[i]["exception"]["message"].SetString(message.c_str(), errorReportForm.GetAllocator());

			logReports[i]["properties"]["exception"].SetString(exceptionProp.str().c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["message"].SetString(message.c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["culture"].SetString(to_string(localeDisplayName).c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["availableMemory"].SetString(to_string((FLOAT)MemoryManager::AppMemoryUsageLimit() / 1024 / 1024).c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["firstUseTimeUTC"].SetString(getTimeStamp("%m/%d/%Y %T").c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["osArchitecture"].SetString(to_string(Package::Current().Id().Architecture()).c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["osVersion"].SetString(osBuild.c_str(), errorReportForm.GetAllocator());
			logReports[i]["properties"]["isElevated"].SetString(isElevatedProcess() ? "True" : "False", errorReportForm.GetAllocator());
		}
		else if (i == 1)
		{
			logReports[i]["data"].SetString(errorAttachment.c_str(), errorReportForm.GetAllocator());
			logReports[i]["errorId"].SetString(crashReportId.c_str(), errorReportForm.GetAllocator());
			logReports[i]["id"].SetString(errorAttachmentId.c_str(), errorReportForm.GetAllocator());
		}
	}


	StringBuffer errorReport;
	Writer<StringBuffer> writer(errorReport);
	errorReportForm.Accept(writer);

	CURL* curl = curl_easy_init();
	if (curl) {
		curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headerList);
		curl_easy_setopt(curl, CURLOPT_URL, APPCENTER_ENDPOINT);
		curl_easy_setopt(curl, CURLOPT_POSTFIELDS, errorReport.GetString());

#ifdef  _DEBUG
		curl_easy_setopt(curl, CURLOPT_VERBOSE, 1L);
#endif

		CURLcode res = curl_easy_perform(curl);
		if (res != CURLE_OK)
			fprintf(stderr, "curl_easy_perform() failed: %s\n", curl_easy_strerror(res));
	}
	curl_easy_cleanup(curl);
}