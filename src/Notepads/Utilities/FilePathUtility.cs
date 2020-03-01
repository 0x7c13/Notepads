namespace Notepads.Utilities
{
    using System.IO;

    public static class FilePathUtility
    {
        //public static string TrimFilePathForDisplay(string filePath, int maxLength)
        //{
        //    if (filePath.Length <= maxLength) return filePath;

        //    var fileName = Path.GetFileName(filePath);

        //    if (fileName.Length >= maxLength){
        //        return fileName.Substring(0, maxLength);
        //    }

        //    if (fileName.Length + @"...\".Length > maxLength)
        //    {
        //        return fileName;
        //    }

        //    var rootLength = maxLength - @"...\".Length - fileName.Length;

        //    if (rootLength > 0) {
        //        return filePath.Substring(0, rootLength) + @"...\" + fileName;
        //    }
        //    else
        //    {
        //        return @"...\" + fileName;
        //    }
        //}
    }
}