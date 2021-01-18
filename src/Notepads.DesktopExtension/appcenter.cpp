// Documentation is at https://docs.microsoft.com/en-us/appcenter/diagnostics/upload-crashes
#include "pch.h"
#include "appcenter.h"
#include "log.h"
#include "rapidjson/stringbuffer.h"

#define APPCENTER_ENDPOINT "https://in.appcenter.ms/logs?Api-Version=1.0.0"

using namespace fmt;
using namespace rapidjson;
using namespace std;
using namespace winrt;
using namespace AppCenter;
using namespace Windows::ApplicationModel;
using namespace Windows::Storage;
using namespace Windows::System;

IInspectable readSettingsKey(hstring key)
{
	return ApplicationData::Current().LocalSettings().Values().TryLookup(key);
}

VOID AppCenter::start()
{
	if (!AppCenterSecret || wcslen(AppCenterSecret) == 0) return;

	hstring installId = unbox_value_or<hstring>(readSettingsKey(AppCenterInstallIdStr), L"");
	if (installId.empty()) return;

	if (!headerList)
	{
		headerList = curl_slist_append(headerList, "Content-Type: application/json");
		headerList = curl_slist_append(headerList, format("app-secret: {}", to_string(AppCenterSecret)).c_str());
		headerList = curl_slist_append(headerList, format("install-id: {}", to_string(installId)).c_str());
	}

	if (!deviceInfo)
	{
		deviceInfo = new Device();
	}
}

VOID AppCenter::trackError(bool isFatal, DWORD errorCode, const string& message, const stacktrace& stackTrace)
{
	if (!headerList) return;

	TCHAR locale[LOCALE_NAME_MAX_LENGTH + 1];
	LCIDToLocaleName(GetThreadLocale(), locale, LOCALE_NAME_MAX_LENGTH, 0);
	TCHAR localeDisplayName[LOCALE_NAME_MAX_LENGTH + 1];
	GetLocaleInfoEx(locale, LOCALE_SENGLISHDISPLAYNAME, localeDisplayName, LOCALE_NAME_MAX_LENGTH);

	string isElevated = isElevatedProcess() ? "True" : "False";
	string eventType = isFatal ? "OnWin32UnhandledException" : "OnWin32UnexpectedException";

	string attachmentData = base64_encode(format("Exception: Win32Exception code no. {}\nMessage: {}\nIsDesktopExtension: True, IsElevated: {}",
												errorCode, message, isElevated));

	vector<pair<const CHAR*, string>> properties
	{
		pair("Exception", format("Win32Exception: Exception of code no. {} was thrown.", errorCode)),
		pair("Message", message),
		pair("Culture", to_string(localeDisplayName)),
		pair("AvailableMemory", to_string((FLOAT)MemoryManager::AppMemoryUsageLimit() / 1024 / 1024)),
		pair("FirstUseTimeUTC", getTimeStamp("%m/%d/%Y %T")),
		pair("OSArchitecture", to_string(Package::Current().Id().Architecture())),
		pair("OSVersion", deviceInfo->getOsVersion()),
		pair("IsDesktopExtension", "True"),
		pair("IsElevated", isElevated)
	};

	vector<Log> errorReportSet
	{
		Log(LogType::managedError, isFatal, new Exception(message, stackTrace)),
		Log(LogType::handledError, isFatal, new Exception(message), properties)
	};
	errorReportSet.insert(errorReportSet.end(),
		{
			Log(LogType::errorAttachment, errorReportSet[0].Id(), attachmentData),
			Log(LogType::errorAttachment, errorReportSet[1].Id(), attachmentData),
			Log(LogType::event, errorReportSet[0].Sid(), eventType, properties)
		});

	StringBuffer errorReport;
#ifdef  _DEBUG
	PrettyWriter<StringBuffer> writer(errorReport);
#else
	Writer<StringBuffer> writer(errorReport);
#endif

	writer.StartObject();
	writer.String("logs");
	writer.StartArray();
	for (vector<Log>::const_iterator report = errorReportSet.begin(); report != errorReportSet.end(); ++report)
	{
		report->Serialize(writer);
	}
	writer.EndArray();
	writer.EndObject();

	CURL* curl = curl_easy_init();
	if (curl)
	{
		curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headerList);
		curl_easy_setopt(curl, CURLOPT_URL, APPCENTER_ENDPOINT);
		curl_easy_setopt(curl, CURLOPT_POSTFIELDS, errorReport.GetString());

#ifdef  _DEBUG
		curl_easy_setopt(curl, CURLOPT_VERBOSE, 1L);
#endif

		CURLcode res = curl_easy_perform(curl);
		if (res != CURLE_OK)
		{
			printDebugMessage(format(L"curl_easy_perform() failed: {}", to_hstring(curl_easy_strerror(res))).c_str());
		}
	}
	curl_easy_cleanup(curl);
}

VOID AppCenter::trackEvent(const string& name, const vector<pair<const CHAR*, string>>& properties, const string& sid)
{
	if (!headerList) return;

	StringBuffer eventReport;
#ifdef  _DEBUG
	PrettyWriter<StringBuffer> writer(eventReport);
#else
	Writer<StringBuffer> writer(eventReport);
#endif

	writer.StartObject();
	writer.String("logs");
	writer.StartArray();
	Log(LogType::event, sid, name, properties).Serialize(writer);
	writer.EndArray();
	writer.EndObject();

	CURL* curl = curl_easy_init();
	if (curl)
	{
		curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headerList);
		curl_easy_setopt(curl, CURLOPT_URL, APPCENTER_ENDPOINT);
		curl_easy_setopt(curl, CURLOPT_POSTFIELDS, eventReport.GetString());

#ifdef  _DEBUG
		curl_easy_setopt(curl, CURLOPT_VERBOSE, 1L);
#endif

		CURLcode res = curl_easy_perform(curl);
		if (res != CURLE_OK)
		{
			printDebugMessage(format(L"curl_easy_perform() failed: {}", to_hstring(curl_easy_strerror(res))).c_str());
		}
	}
	curl_easy_cleanup(curl);
}

VOID AppCenter::exit()
{
	if (headerList) LocalFree(headerList);
	if (deviceInfo) LocalFree(deviceInfo);
}