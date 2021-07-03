#pragma once
#include "pch.h"
#include "logger.h"

#define APP_CENTER_FORMAT L"{year.full}-{month.integer(2)}-{day.integer(2)}" \
						   "T{hour.integer(2)}:{minute.integer(2)}:{second.integer(2)}Z"

const winrt::hstring launch_time_stamp = logger::get_utc_time_stamp(APP_CENTER_FORMAT);

__declspec(selectany) winrt::hstring last_error_report_sid = L"";
__declspec(selectany) device device_info {};

struct report
{
	using json_object = winrt::Windows::Data::Json::JsonObject;
	using dictionary = std::vector<std::pair<winrt::hstring, winrt::hstring>>;

	virtual winrt::Windows::Data::Json::IJsonValue to_json() const noexcept
	{
		auto json_obj = json_object();
		json_obj.Insert(L"type", JsonValue::CreateStringValue(type()));
		json_obj.Insert(L"id", JsonValue::CreateStringValue(m_id));
		json_obj.Insert(L"timestamp", JsonValue::CreateStringValue(m_timestamp));
		json_obj.Insert(L"appLaunchTimestamp", JsonValue::CreateStringValue(launch_time_stamp));
		json_obj.Insert(L"architecture", JsonValue::CreateStringValue(m_arch));

		// Write report specific data
		append_additional_data(json_obj);

		json_obj.Insert(L"device", device_info.to_json());

		return json_obj;
	}

	static void add_device_metadata(dictionary& properties) noexcept
	{
		properties.insert(
			properties.end(),
			{
				std::pair(L"AvailableMemory", winrt::to_hstring((float)MemoryManager::AppMemoryUsageLimit() / 1024 / 1024)),
				std::pair(L"OSArchitecture", to_hstring(Package::Current().Id().Architecture())),
				std::pair(L"OSVersion", device_info.osbuild()),
				std::pair(L"IsDesktopExtension", L"True"),
				std::pair(L"IsElevated", is_elevated_process() ? L"True" : L"False")
			}
		);
	}

protected:

	using hstring = winrt::hstring;
	using json_array = winrt::Windows::Data::Json::JsonArray;

	report() noexcept
	{
		std::wstring id = winrt::to_hstring(winrt::Windows::Foundation::GuidHelper::CreateNewGuid()).c_str();
		id.erase(0, id.find_first_not_of('{')).erase(id.find_last_not_of('}') + 1);
		m_id = id;
	}

	report(report const& other) noexcept :
		m_id(other.m_id),
		m_process(other.m_process),
		m_arch(other.m_arch),
		m_timestamp(other.m_timestamp)
	{
	}

	report(report&&) noexcept = default;

	virtual hstring type() const noexcept
	{
		return L"";
	}

	virtual void append_additional_data(json_object& /* writer */) const noexcept
	{
		// override in child classes
	}

	hstring m_id;
	hstring m_process = L"Notepads32.exe";
	hstring m_arch = to_hstring(Package::Current().Id().Architecture());
	hstring m_timestamp = logger::get_utc_time_stamp(APP_CENTER_FORMAT);

private:

	using Package = winrt::Windows::ApplicationModel::Package;
	using JsonValue = winrt::Windows::Data::Json::JsonValue;
	using MemoryManager = winrt::Windows::System::MemoryManager;
	using ProcessorArchitecture = winrt::Windows::System::ProcessorArchitecture;

	static winrt::hstring to_hstring(ProcessorArchitecture arch) noexcept
	{
		switch (arch)
		{
		case ProcessorArchitecture::Arm:
			return L"Arm";
		case ProcessorArchitecture::Arm64:
			return L"Arm64";
		case ProcessorArchitecture::X86OnArm64:
			return L"X86OnArm64";
		case ProcessorArchitecture::X86:
			return L"X86";
		case ProcessorArchitecture::X64:
			return L"X64";
		case ProcessorArchitecture::Neutral:
			return L"Neutral";
		default:
			return L"Unknown";
		}
	}
};