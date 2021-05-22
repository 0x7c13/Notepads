#pragma once
#include "pch.h"
#include "constants.h"
#include "appcenter.h"

struct device
{
	using Package = winrt::Windows::ApplicationModel::Package;
	using AnalyticsInfo = winrt::Windows::System::Profile::AnalyticsInfo;
	using GlobalizationPreferences = winrt::Windows::System::UserProfile::GlobalizationPreferences;
	using EasClientDeviceInformation = winrt::Windows::Security::ExchangeActiveSyncProvisioning::EasClientDeviceInformation;

	device() noexcept
	{
		auto package_version = Package::Current().Id().Version();
		m_build = m_version = fmt::format(
			"{}.{}.{}.{}",
			package_version.Major,
			package_version.Minor,
			package_version.Build,
			package_version.Revision
		);

		EasClientDeviceInformation oem_info{};
		m_os = to_string(oem_info.OperatingSystem());

		auto version = std::stoull(AnalyticsInfo::VersionInfo().DeviceFamilyVersion().c_str());
		auto major = (version & 0xFFFF000000000000L) >> 48;
		auto minor = (version & 0x0000FFFF00000000L) >> 32;
		auto build = (version & 0x00000000FFFF0000L) >> 16;
		auto revision = (version & 0x000000000000FFFFL);
		m_osversion = fmt::format("{}.{}.{}", major, minor, build);
		m_osbuild = fmt::format("{}.{}.{}.{}", major, minor, build, revision);

		m_model = to_string(oem_info.SystemProductName());
		m_oem = to_string(oem_info.SystemManufacturer());

		RECT desktop;
		GetWindowRect(GetDesktopWindow(), &desktop);
		m_screen = fmt::format("{}x{}", desktop.right, desktop.bottom);

		m_locale = to_string(
			GlobalizationPreferences::Languages().Size() > 0
			? GlobalizationPreferences::Languages().First().Current()
			: L""
		);

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

	template <typename Writer>
	void serialize(Writer& writer) const noexcept
	{
		writer.StartObject();
		writer.String("appNamespace");
		writer.String(m_namespace.c_str(), static_cast<rapidjson::SizeType>(m_namespace.length()));
		writer.String("appVersion");
		writer.String(m_version.c_str(), static_cast<rapidjson::SizeType>(m_version.length()));
		writer.String("appBuild");
		writer.String(m_build.c_str(), static_cast<rapidjson::SizeType>(m_build.length()));
		writer.String("sdkName");
		writer.String(m_sdk.c_str(), static_cast<rapidjson::SizeType>(m_sdk.length()));
		writer.String("sdkVersion");
		writer.String(m_sdkversion.c_str(), static_cast<rapidjson::SizeType>(m_sdkversion.length()));
		writer.String("osName");
		writer.String(m_os.c_str(), static_cast<rapidjson::SizeType>(m_os.length()));
		writer.String("osVersion");
		writer.String(m_osversion.c_str(), static_cast<rapidjson::SizeType>(m_osversion.length()));
		writer.String("osBuild");
		writer.String(m_osbuild.c_str(), static_cast<rapidjson::SizeType>(m_osbuild.length()));
		writer.String("model");
		writer.String(m_model.c_str(), static_cast<rapidjson::SizeType>(m_model.length()));
		writer.String("oemName");
		writer.String(m_oem.c_str(), static_cast<rapidjson::SizeType>(m_oem.length()));
		writer.String("screenSize");
		writer.String(m_screen.c_str(), static_cast<rapidjson::SizeType>(m_screen.length()));
		writer.String("locale");
		writer.String(m_locale.c_str(), static_cast<rapidjson::SizeType>(m_locale.length()));
		writer.String("timeZoneOffset");
		writer.Uint(m_timezone);
		writer.EndObject();
	}

	std::string osbuild() const noexcept
	{
		return m_osbuild;
	}

private:
	std::string m_namespace = "Notepads.DesktopExtension";
	std::string m_version;
	std::string m_build;
	std::string m_sdk = "appcenter.uwp";
	std::string m_sdkversion = APP_CENTER_SDK_VERSION;
	std::string m_os;
	std::string m_osversion;
	std::string m_osbuild;
	std::string m_model;
	std::string m_oem;
	std::string m_screen;
	std::string m_locale;
	unsigned m_timezone;
};