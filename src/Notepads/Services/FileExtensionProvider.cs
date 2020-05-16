namespace Notepads.Services
{
    using System.Collections.Generic;

    public static class FileExtensionProvider
    {
        public static ISet<string> TextDocumentFileExtensions { get; } = new HashSet<string>()
        {
            ".txt", ".md", ".markdown"
        };

        public static ISet<string> AllSupportedFileExtensions { get; } = new HashSet<string>()
        {
            ".txt", ".md", ".markdown", ".mkd", ".mdwn", ".mdown", ".markdn", ".mdtxt", ".workbook",
            ".csv",
            ".cfg", ".config", ".cnf", ".conf", ".properties", ".directory", ".ini", ".log", ".v", ".nfo", ".ahk",
            ".json", ".jsonc", ".jsonld", ".hintrc", ".babelrc", ".eslintrc", ".jslintrc", ".bowerrc", ".jscsrc", ".webmanifest", ".har", ".yml", ".yaml",
            ".xml", ".xsd", ".ascx", ".atom", ".axml", ".bpmn", ".cpt", ".csl",
            ".xaml",
            ".html", ".htm", ".shtml", ".xhtml", ".mdoc", ".jshtm", ".volt", ".asp", ".aspx", ".jsp", ".jspx", ".css", ".scss", ".vue", ".vuerc", ".cgi",
            ".gitignore", ".gitconfig", ".gitattributes",
            ".ps1", ".psm1", ".psd1", ".pssc", ".psrc",
            ".sh", ".bashrc", ".rc", ".bash", ".bash_aliases", ".bash_history", ".bash_profile", ".bash_login", ".bash_logout", ".vimrc", ".zshrc", ".zsh_history",
            ".c", ".i", ".ii", ".cmake", ".h", ".hh", ".hpp", ".hxx", ".cpp", ".cxx", ".cc", ".m", ".mm", ".ino", ".cs", ".csx", ".cake",
            ".rs",
            ".go",
            ".swift",
            ".php", ".php4", ".php5", ".phtml", ".ctp",
            ".py", ".vb", ".vbs", ".brs", ".bas",
            ".java", ".jav",
            ".sql", ".dsql",
            ".rb", ".rbx", ".rjs", ".gemspec", ".rake", ".ru", ".erb", ".rbi", ".arb",
            ".pl", ".pm", ".psgi",
            ".p6", ".pl6", ".pm6", ".nqp",
            ".fs", ".fsi", ".fsx", ".fsscript",
            ".js", ".jsx", ".es6", ".mjs", ".cjs", ".pac", ".ts", ".tsx", ".lua",
            ".groovy", ".gvy", ".gradle",
            ".clj", ".cljs", ".cljc", ".cljx", ".clojure", ".edn",
            ".r", ".rhistory", ".rprofile", ".rt",
            ".jade", ".pug",
            ".coffee", ".cson", ".iced",
            ".handlebars", ".hbs", ".hjs",
            ".srt", ".ass", ".ssa", ".lrc",
            ".patch", ".diff", ".rej",
            ".project", ".prj", ".npmrc", ".buildpath",
            ".hlsl", ".hlsli", ".fx", ".fxh", ".vsh", ".psh", ".cginc", ".compute",
            ".cshtml",
            ".dockerfile",
            ".asm",
            ".shader",
            ".glsp",
            ".dat",
            ".map",
            ".less",
            ".bond",
            ".t",
            ".install",
            ".profile",
            ".ebuild",
            ".user",
            ".pod", ".podspec",
        };

        public static bool IsFileExtensionSupported(string fileExtension)
        {
            if (string.IsNullOrEmpty(fileExtension))
            {
                return false;
            }

            if (!fileExtension.StartsWith("."))
            {
                fileExtension = "." + fileExtension;
            }

            return AllSupportedFileExtensions.Contains(fileExtension.ToLower());
        }
    }
}