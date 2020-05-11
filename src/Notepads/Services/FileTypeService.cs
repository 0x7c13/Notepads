namespace Notepads.Services
{
    using System.Collections.Generic;

    public static class FileTypeService
    {
        public static IList<string> TextDocumentFileExtensions { get; } = new List<string>() { ".txt", ".md", ".markdown" };

        public static IList<string> AllSupportedFileExtensions { get; } = new List<string>()
        {
            ".txt", ".md", ".markdown", ".csv",
            ".cfg", ".config", ".cnf", ".conf", ".ini", ".log", ".v", ".nfo", ".ahk",
            ".json", ".yml", ".yaml", ".xml", ".xaml",
            ".html", ".htm", ".asp", ".aspx", ".jsp", ".jspx", ".css", ".scss", ".vue", ".vuerc", ".cgi",
            ".gitignore", ".gitconfig", ".gitattributes",
            ".ps1", ".bat", ".cmd", ".vbs", ".sh", ".bashrc", ".rc", ".bash", ".bash_history", ".vimrc", ".zshrc", ".zsh_history",
            ".c", ".cmake", ".h", ".hpp", ".cpp", ".cc", ".cs", ".m", ".mm", ".php", ".py", ".rb", ".vb", ".java", ".go", ".pl", ".sql",
            ".js", ".ts", ".lua",
            ".srt", ".ass", ".ssa", ".lrc",
            ".project", ".prj", ".npmrc", ".buildpath",
        };
    }
}