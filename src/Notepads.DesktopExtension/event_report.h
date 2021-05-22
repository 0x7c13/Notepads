#pragma once
#include "pch.h"
#include "error.h"
#include "report.h"

struct event_report : report
{
	explicit event_report(std::string const& name, report::dictionary const& properties) noexcept :
		report(), m_sid(last_error_report_sid), m_name(name), m_properties(properties)
	{
		last_error_report_sid = "";
	}

	event_report(event_report const& other) noexcept :
		report(other),
		m_sid(other.m_sid),
		m_name(other.m_name),
		m_properties(other.m_properties)
	{
	}

	event_report(event_report&& other) noexcept :
		report(other)
	{
	}

protected:

	virtual void append_type_data(json_writer& writer)  const noexcept
	{
		writer.String("type");
		writer.String("event");
	}

	virtual void append_additional_data(json_writer& writer)  const noexcept
	{
		report::append_additional_data(writer);

		writer.String("sid");
		writer.String(m_sid.c_str(), static_cast<rapidjson::SizeType>(m_sid.length()));
		writer.String("name");
		writer.String(m_name.c_str(), static_cast<rapidjson::SizeType>(m_name.length()));

		// Write custom properties if available
		if (!m_properties.empty())
		{
			writer.String("properties");
			writer.StartObject();
			for (auto& property : m_properties)
			{
				writer.String(property.first.c_str(), static_cast<rapidjson::SizeType>(property.first.length()));
				writer.String(property.second.c_str(), static_cast<rapidjson::SizeType>(property.second.length()));
			}
			writer.EndObject();
		}
	}

	virtual void append_report(json_writer& writer)  const noexcept
	{
		report::append_report(writer);
	}

	std::string m_sid;
	std::string m_name;
	report::dictionary m_properties;
};