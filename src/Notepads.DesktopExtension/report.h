#pragma once
#include "pch.h"
#include "logger.h"

#define APP_CENTER_FORMAT L"{year.full}-{month.integer(2)}-{day.integer(2)}" \
						   "T{hour.integer(2)}:{minute.integer(2)}:{second.integer(2)}Z"

const std::string launch_time_stamp = winrt::to_string(
	logger::get_utc_time_stamp(APP_CENTER_FORMAT)
);

__declspec(selectany) std::string last_error_report_sid = "";
__declspec(selectany) device device_info {};

struct report
{
#ifdef  _DEBUG
	using json_writer = rapidjson::PrettyWriter<rapidjson::StringBuffer>;
#else
	using json_writer = rapidjson::Writer<rapidjson::StringBuffer>;
#endif
	using Package = winrt::Windows::ApplicationModel::Package;
	using MemoryManager = winrt::Windows::System::MemoryManager;
	using dictionary = std::vector<std::pair<std::string, std::string>>;
	using ProcessorArchitecture = winrt::Windows::System::ProcessorArchitecture;

	virtual void serialize(json_writer& writer)  const noexcept
	{
		writer.StartObject();

		// Write report type data
		append_type_data(writer);

		writer.String("id");
		writer.String(m_id.c_str(), static_cast<rapidjson::SizeType>(m_id.length()));
		writer.String("timestamp");
		writer.String(m_timestamp.c_str(), static_cast<rapidjson::SizeType>(m_timestamp.length()));
		writer.String("appLaunchTimestamp");
		writer.String(launch_time_stamp.c_str(), static_cast<rapidjson::SizeType>(launch_time_stamp.length()));
		writer.String("architecture");
		writer.String(m_arch.c_str(), static_cast<rapidjson::SizeType>(m_arch.length()));

		// Write report specific data
		append_additional_data(writer);

		// Write device specific data
		writer.String("device");
		device_info.serialize(writer);

		writer.EndObject();

		// Write additional report data
		append_report(writer);
	}

	static std::string to_string(ProcessorArchitecture arch) noexcept
	{
		switch (arch)
		{
		case ProcessorArchitecture::Arm:
			return "Arm";
		case ProcessorArchitecture::Arm64:
			return "Arm64";
		case ProcessorArchitecture::X86OnArm64:
			return "X86OnArm64";
		case ProcessorArchitecture::X86:
			return "X86";
		case ProcessorArchitecture::X64:
			return "X64";
		case ProcessorArchitecture::Neutral:
			return "Neutral";
		default:
			return "Unknown";
		}
	}

	static void add_device_metadata(dictionary& properties) noexcept
	{
		properties.insert(
			properties.end(),
			{
				std::pair("AvailableMemory", std::to_string((float)MemoryManager::AppMemoryUsageLimit() / 1024 / 1024)),
				std::pair("OSArchitecture", to_string(Package::Current().Id().Architecture())),
				std::pair("OSVersion",device_info.osbuild()),
				std::pair("IsDesktopExtension", "True"),
				std::pair("IsElevated", is_elevated_process() ? "True" : "False")
			}
		);
	}

protected:

	report() noexcept
	{
		m_id = winrt::to_string(winrt::to_hstring(winrt::Windows::Foundation::GuidHelper::CreateNewGuid()));
		m_id.erase(0, m_id.find_first_not_of('{')).erase(m_id.find_last_not_of('}') + 1);
	}

	report(report const& other) noexcept :
		m_id(other.m_id),
		m_process(other.m_process),
		m_arch(other.m_arch),
		m_timestamp(other.m_timestamp)
	{
	}

	report(report&&) noexcept = default;

	virtual void append_type_data(json_writer& /* writer */)  const noexcept
	{
		// override in child classes
	}

	virtual void append_additional_data(json_writer& /* writer */)  const noexcept
	{
		// override in child classes
	}

	virtual void append_report(json_writer& /* writer */)  const noexcept
	{
		// override in child classes
	}

	std::string m_id;
	std::string m_process = "Notepads32.exe";
	std::string m_arch = to_string(Package::Current().Id().Architecture());
	std::string m_timestamp = winrt::to_string(logger::get_utc_time_stamp(APP_CENTER_FORMAT));
};