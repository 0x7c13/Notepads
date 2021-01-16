#pragma once
#include "pch.h"

#define IGNORED_FRAME_COUNT 6

namespace AppCenter
{
	using namespace boost::stacktrace;
	using namespace std;

	class Frame
	{
	public:
		Frame(const frame& frame) : _methodName(frame.name()), _fileName(frame.source_file()), _lineNumber(frame.source_line()) {}
		Frame(const Frame& frame) : _methodName(frame._methodName), _fileName(frame._fileName), _lineNumber(frame._lineNumber) {}

		~Frame() {}

		Frame& operator=(const Frame& frame)
		{
			if (this == &frame) return *this;

			_methodName = frame._methodName;
			_fileName = frame._fileName;
			_lineNumber = frame._lineNumber;
			return *this;
		}

		template <typename Writer>
		VOID Serialize(Writer& writer) const
		{
			if (_lineNumber <= 0 && _fileName.empty()) return;

			writer.StartObject();

			writer.String("methodName");
			writer.String(_methodName.c_str(), static_cast<SizeType>(_methodName.length()));
			writer.String("fileName");
			writer.String(_fileName.c_str(), static_cast<SizeType>(_fileName.length()));
			writer.String("lineNumber");
			writer.Uint(_lineNumber);

			writer.EndObject();
		}

	private:
		string _methodName;
		string _fileName;
		unsigned _lineNumber;
	};

	class Exception
	{
	public:
		Exception(const string& message) : _message(message) {}
		Exception(const string& message, const string& stackTrace) : _message(message), _stackTrace(stackTrace) {}
		Exception(const Exception& exception) : _message(exception._message), _stackTrace(exception._stackTrace), _frames(exception._frames) {}
		Exception(const exception& exception) : _message(exception.what()) {}

		Exception(const string& message, const stacktrace& stackTrace) : _message(message)
		{
			_frames.assign(stackTrace.as_vector().begin() + IGNORED_FRAME_COUNT, stackTrace.as_vector().end());
		}

		~Exception() {}

		Exception& operator=(const Exception& exception)
		{
			if (this == &exception) return *this;

			_type = exception._type;
			_message = exception._message;
			_stackTrace = exception._stackTrace;
			_frames = exception._frames;
			return *this;
		}

		template <typename Writer>
		VOID Serialize(Writer& writer) const
		{
			writer.StartObject();

			writer.String("type");
			writer.String(_type.c_str(), static_cast<SizeType>(_type.length()));
			writer.String("message");
			writer.String(_message.c_str(), static_cast<SizeType>(_message.length()));

			if (!_stackTrace.empty())
			{
				writer.String("stackTrace");
				writer.String(_stackTrace.c_str(), static_cast<SizeType>(_stackTrace.length()));
			}

			if (!_frames.empty())
			{
				writer.String("frames");
				writer.StartArray();
				for (Frame frame : _frames)
				{
					frame.Serialize(writer);
				}
				writer.EndArray();
			}

			writer.EndObject();
		}

	private:
		string _type = "Win32Exception";
		string _message;
		string _stackTrace = "";
		vector<Frame> _frames;
	};
}