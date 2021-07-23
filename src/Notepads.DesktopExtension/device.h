#pragma once
#include "pch.h"
#include "constants.h"

struct device
{
	device() noexcept
	{
		auto package_version = Package::Current().Id().Version();
		m_build = m_version = std::format(
			L"{}.{}.{}.{}",
			package_version.Major,
			package_version.Minor,
			package_version.Build,
			package_version.Revision
		);

		EasClientDeviceInformation oem_info{};
		m_os = oem_info.OperatingSystem();

		auto version = std::stoull(AnalyticsInfo::VersionInfo().DeviceFamilyVersion().c_str());
		auto major = (version & 0xFFFF000000000000L) >> 48;
		auto minor = (version & 0x0000FFFF00000000L) >> 32;
		auto build = (version & 0x00000000FFFF0000L) >> 16;
		auto revision = (version & 0x000000000000FFFFL);
		m_osversion = std::format(L"{}.{}.{}", major, minor, build);
		m_osbuild = std::format(L"{}.{}.{}.{}", major, minor, build, revision);

		m_model = oem_info.SystemProductName();
		m_oem = oem_info.SystemManufacturer();

		RECT desktop;
		GetWindowRect(GetDesktopWindow(), &desktop);
		m_screen = std::format(L"{}x{}", desktop.right, desktop.bottom);

		m_locale = GlobalizationPreferences::Languages().Size() > 0
			? GlobalizationPreferences::Languages().First().Current()
			: L"";

		TIME_ZONE_INFORMATION timeZoneInfo;
		GetTimeZoneInformation(&timeZoneInfo);
		m_timezone = -1 * timeZoneInfo.Bias;
	}

	device(device const& device) noexcept :
		m_version(device.m_version),
		m_build(device.m_build),
		m_sdkversion(device.m_sdkversion),
		m_os(device.m_os),
		m_osversion(device.m_osversion),
		m_osbuild(device.m_osbuild),
		m_model(device.m_model),
		m_oem(device.m_oem),
		m_screen(device.m_screen),
		m_locale(device.m_locale),
		m_timezone(device.m_timezone)
	{}

	device& operator=(device const& device) noexcept
	{
		m_namespace = device.m_namespace;
		m_version = device.m_version;
		m_build = device.m_build;
		m_sdk = device.m_sdk;
		m_sdkversion = device.m_sdkversion;
		m_os = device.m_os;
		m_osversion = device.m_osversion;
		m_osbuild = device.m_osbuild;
		m_model = device.m_model;
		m_oem = device.m_oem;
		m_screen = device.m_screen;
		m_locale = device.m_locale;
		m_timezone = device.m_timezone;
		return *this;
	}

	winrt::Windows::Data::Json::IJsonValue to_json() const noexcept
	{
		auto json_obj = winrt::Windows::Data::Json::JsonObject();
		json_obj.Insert(L"appNamespace", JsonValue::CreateStringValue(m_namespace));
		json_obj.Insert(L"appVersion", JsonValue::CreateStringValue(m_version));
		json_obj.Insert(L"appBuild", JsonValue::CreateStringValue(m_build));
		json_obj.Insert(L"sdkName", JsonValue::CreateStringValue(m_sdk));
		json_obj.Insert(L"sdkVersion", JsonValue::CreateStringValue(m_sdkversion));
		json_obj.Insert(L"osName", JsonValue::CreateStringValue(m_os));
		json_obj.Insert(L"osVersion", JsonValue::CreateStringValue(m_osversion));
		json_obj.Insert(L"osBuild", JsonValue::CreateStringValue(m_osbuild));
		json_obj.Insert(L"model", JsonValue::CreateStringValue(m_model));
		json_obj.Insert(L"oemName", JsonValue::CreateStringValue(m_oem));
		json_obj.Insert(L"screenSize", JsonValue::CreateStringValue(m_screen));
		json_obj.Insert(L"locale", JsonValue::CreateStringValue(m_locale));
		json_obj.Insert(L"timeZoneOffset", JsonValue::CreateNumberValue(m_timezone));
		return json_obj;
	}

	winrt::hstring osbuild() const noexcept
	{
		return m_osbuild;
	}

private:
	using hstring = winrt::hstring;
	using Package = winrt::Windows::ApplicationModel::Package;
	using JsonValue = winrt::Windows::Data::Json::JsonValue;
	using AnalyticsInfo = winrt::Windows::System::Profile::AnalyticsInfo;
	using GlobalizationPreferences = winrt::Windows::System::UserProfile::GlobalizationPreferences;
	using EasClientDeviceInformation = winrt::Windows::Security::ExchangeActiveSyncProvisioning::EasClientDeviceInformation;

	hstring m_namespace = L"Notepads.DesktopExtension";
	hstring m_version;
	hstring m_build;
	hstring m_sdk = L"appcenter.uwp";
	hstring m_sdkversion = APP_CENTER_SDK_VERSION;
	hstring m_os;
	hstring m_osversion;
	hstring m_osbuild;
	hstring m_model;
	hstring m_oem;
	hstring m_screen;
	hstring m_locale;
	unsigned m_timezone;
};