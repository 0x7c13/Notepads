#pragma once
#include "pch.h"
#include "error.h"
#include "managed_error_report.h"

struct handled_error_report : managed_error_report
{
	explicit handled_error_report(
		winrt_error const& error,
		report::dictionary const& properties,
		std::string const& attachment = ""
	) noexcept :
		managed_error_report(error, attachment), m_properties(properties)
	{
	}

	handled_error_report(handled_error_report const& other) noexcept :
		managed_error_report(other),
		m_properties(other.m_properties)
	{
	}

	handled_error_report(handled_error_report&& other) noexcept :
		managed_error_report(other)
	{
	}

protected:

	virtual void append_type_data(json_writer& writer)  const noexcept
	{
		writer.String("type");
		writer.String("handledError");
	}

	virtual void append_additional_data(json_writer& writer)  const noexcept
	{
		managed_error_report::append_additional_data(writer);

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

	/*virtual void append_report(json_writer& writer)  const noexcept
	{
		report::append_report(writer);

		if (!m_attachment.empty())
		{
			error_attachment_report(m_id, m_attachment).serialize(writer);
		}
	}*/

	report::dictionary m_properties;
};