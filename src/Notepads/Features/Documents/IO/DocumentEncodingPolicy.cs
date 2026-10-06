// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using Notepads.Features.Documents.Text;
using Notepads.Infrastructure.Diagnostics;
using UtfUnknown;

namespace Notepads.Features.Documents.IO;

internal static class DocumentEncodingPolicy
{
    public static bool TryGuessEncoding(Stream stream, out Encoding encoding)
    {
        encoding = null;

        try
        {
            var result = CharsetDetector.DetectFromStream(stream);
            if (result.Detected?.Encoding != null) // Detected can be null
            {
                encoding = AnalyzeAndGuessEncoding(result);
                return true;
            }
            else if (stream.Length > 0) // We do not care about empty file
            {
                AnalyticsService.TrackEvent("UnableToDetectEncoding");
            }
        }
        catch (Exception ex)
        {
            AnalyticsService.TrackEvent("TryGuessEncodingFailedWithException", new Dictionary<string, string>()
            {
                { "Exception", ex.ToString() },
                { "Message", ex.Message }
            });
        }

        return false;
    }

    private static Encoding AnalyzeAndGuessEncoding(DetectionResult result)
    {
        Encoding encoding = result.Detected.Encoding;
        var confidence = result.Detected.Confidence;
        var foundBetterMatch = false;

        // Let's treat ASCII as UTF-8 for better accuracy
        if (EncodingCatalog.Equals(encoding, Encoding.ASCII)) encoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

        // If confidence is above 80%, we should just use it
        if (confidence > 0.80f && result.Details.Count == 1) return encoding;

        // Try find a better match based on User's current Windows ANSI code page
        // Priority: UTF-8 > SystemDefaultANSIEncoding (Codepage: 0) > CurrentCultureANSIEncoding
        if (encoding is not UTF8Encoding)
        {
            foreach (var detail in result.Details)
            {
                if (detail.Confidence <= 0.5f)
                {
                    continue;
                }
                if (detail.Encoding is UTF8Encoding)
                {
                    foundBetterMatch = true;
                }
                else if (EncodingCatalog.TryGetSystemDefaultANSIEncoding(out var systemDefaultEncoding)
                         && EncodingCatalog.Equals(systemDefaultEncoding, detail.Encoding))
                {
                    foundBetterMatch = true;
                }
                else if (EncodingCatalog.TryGetCurrentCultureANSIEncoding(out var currentCultureEncoding)
                         && EncodingCatalog.Equals(currentCultureEncoding, detail.Encoding))
                {
                    foundBetterMatch = true;
                }

                if (foundBetterMatch)
                {
                    encoding = detail.Encoding;
                    confidence = detail.Confidence;
                    break;
                }
            }
        }

        // We should fall back to UTF-8 and give it a try if:
        // 1. Detected Encoding is not UTF-8
        // 2. Detected Encoding is not SystemDefaultANSIEncoding (Codepage: 0)
        // 3. Detected Encoding is not CurrentCultureANSIEncoding
        // 4. Confidence of detected Encoding is below 50%
        if (!foundBetterMatch && confidence < 0.5f)
        {
            encoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
        }

        return encoding;
    }

}
