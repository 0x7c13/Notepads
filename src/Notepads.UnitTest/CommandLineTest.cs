namespace Notepads.UnitTest
{
    using System;
    using System.Collections.Generic;
    using Microsoft.VisualStudio.TestTools.UnitTesting;
    using Notepads.Utilities;

    [TestClass]
    public class CommandLineTest
    {
        private const string cmdArg = "notepads";
        private const string pwshArg = "\"C:\\Users\\Test User\\AppData\\Local\\Microsoft\\WindowsApps\\Notepads.exe\"";

        private const string windowsRoot = "C:";
        private const string wslRoot = @"\\wsl$\Ubuntu";

        private static IEnumerable<string[]> GetTestData(string root, string alias)
        {
            yield return new string[] { $"{root}\\", $"{alias} \"{root}\\1.txt\"", $"{root}\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"{root}\\1 2.txt\"", $"{root}\\1 2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} {root}\\1.txt", $"{root}\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} {root}\\1 2.txt", $"{root}\\1 2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"{root}\\2.txt\"", $"{root}\\2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"{root}\\2 3.txt\"", $"{root}\\2 3.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} {root}\\2.txt", $"{root}\\2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} {root}\\2 3.txt", $"{root}\\2 3.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"\\1.txt\"", $"{root}\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"\\1 2.txt\"", $"{root}\\1 2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \\1.txt", $"{root}\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \\1 2.txt", $"{root}\\1 2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"\\2.txt\"", $"{root}\\2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"\\2 3.txt\"", $"{root}\\2 3.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \\2.txt", $"{root}\\2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \\2 3.txt", $"{root}\\2 3.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"1.txt\"", $"{root}\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"1 2.txt\"", $"{root}\\1 2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} 1.txt", $"{root}\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} 1 2.txt", $"{root}\\1 2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"2.txt\"", $"{root}\\1\\2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"2 3.txt\"", $"{root}\\1\\2 3.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} 2.txt", $"{root}\\1\\2.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} 2 3.txt", $"{root}\\1\\2 3.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"1\\2.txt\"", $"{root}\\1\\2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"1 2\\3 4.txt\"", $"{root}\\1 2\\3 4.txt" };
            yield return new string[] { $"{root}\\", $"{alias} 1\\2.txt", $"{root}\\1\\2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} 1 2\\3 4.txt", $"{root}\\1 2\\3 4.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"2\\3.txt\"", $"{root}\\1\\2\\3.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} \"2 3\\4 5.txt\"", $"{root}\\1\\2 3\\4 5.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} 2\\3.txt", $"{root}\\1\\2\\3.txt" };
            yield return new string[] { $"{root}\\1", $"{alias} 2 3\\4 5.txt", $"{root}\\1\\2 3\\4 5.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"D:\\1.txt\"", $"D:\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"D:\\1\\2.txt\"", $"D:\\1\\2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} D:\\1.txt", $"D:\\1.txt" };
            yield return new string[] { $"{root}\\", $"{alias} D:\\1\\2.txt", $"D:\\1\\2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"D:\\1 2.txt\"", $"D:\\1 2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} \"D:\\1 2\\3 4.txt\"", $"D:\\1 2\\3 4.txt" };
            yield return new string[] { $"{root}\\", $"{alias} D:\\1 2.txt", $"D:\\1 2.txt" };
            yield return new string[] { $"{root}\\", $"{alias} D:\\1 2\\3 4.txt", $"D:\\1 2\\3 4.txt" };
        }

        private static IEnumerable<string[]> GetCommandPromptTestData()
        {
            return GetTestData(windowsRoot, cmdArg);
        }

        private static IEnumerable<string[]> GetPowerShellTestData()
        {
            return GetTestData(windowsRoot, pwshArg);
        }

        private static IEnumerable<string[]> GetWslTestData()
        {
            foreach(var data in GetTestData(wslRoot, pwshArg))
            {
                yield return data;
            }

            yield return new string[] { $"{wslRoot}\\", $"{pwshArg} \"/etc/sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\", $"{pwshArg} \"etc/sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\", $"{pwshArg} /etc/sudo.conf", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\", $"{pwshArg} etc/sudo.conf", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\etc", $"{pwshArg} \"/etc/sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\etc", $"{pwshArg} \"sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\etc", $"{pwshArg} /etc/sudo.conf", $"{wslRoot}\\etc\\sudo.conf" };
            yield return new string[] { $"{wslRoot}\\etc", $"{pwshArg} sudo.conf", $"{wslRoot}\\etc\\sudo.conf" };
        }

        [DataTestMethod]
        [DynamicData(nameof(GetCommandPromptTestData), DynamicDataSourceType.Method)]
        public void CommandLineTestForCommandPrompt(string dir, string arg, string result)
        {
            Assert.AreEqual(
                CommandLineUtility.GetAbsolutePathFromCommandLine(dir, arg),
                result
            );
        }

        [DataTestMethod]
        [DynamicData(nameof(GetPowerShellTestData), DynamicDataSourceType.Method)]
        public void CommandLineTestForPowerShell(string dir, string arg, string result)
        {
            Assert.AreEqual(
                CommandLineUtility.GetAbsolutePathFromCommandLine(dir, arg),
                result
            );
        }

        [DataTestMethod]
        [DynamicData(nameof(GetWslTestData), DynamicDataSourceType.Method)]
        public void CommandLineTestForWsl(string dir, string arg, string result)
        {
            Assert.AreEqual(
                CommandLineUtility.GetAbsolutePathFromCommandLine(dir, arg),
                result
            );
        }
    }
}