namespace Notepads.UnitTest
{
    using System;
    using System.Collections.Generic;
    using Microsoft.VisualStudio.TestTools.UnitTesting;
    using Notepads.Utilities;

    public class CommandLineTestCase
    {
        public readonly string parentDirectory;
        public readonly string argument;
        public readonly string result;

        public CommandLineTestCase(
            string parentDirectory,
            string argument,
            string result)
        {
            this.parentDirectory = parentDirectory;
            this.argument = argument;
            this.result = result;
        }
    }

    [TestClass]
    public class CommandLineTest
    {
        private string cmdArg = "notepads";
        private string pwshArg = "\"C:\\Users\\Test User\\AppData\\Local\\Microsoft\\WindowsApps\\Notepads.exe\"";

        private string windowsRoot = "C:";
        private string wslRoot = @"\\wsl$\Ubuntu";

        [TestMethod]
        public void CommandLineTestForCommandPrompt()
        {
            var testCases = new List<CommandLineTestCase>
            {
                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"{windowsRoot}\\1.txt\"", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"{windowsRoot}\\1 2.txt\"", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} {windowsRoot}\\1.txt", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} {windowsRoot}\\1 2.txt", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"{windowsRoot}\\2.txt\"", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"{windowsRoot}\\2 3.txt\"", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} {windowsRoot}\\2.txt", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} {windowsRoot}\\2 3.txt", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"\\1.txt\"", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"\\1 2.txt\"", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \\1.txt", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \\1 2.txt", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"\\2.txt\"", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"\\2 3.txt\"", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \\2.txt", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \\2 3.txt", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"1.txt\"", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"1 2.txt\"", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} 1.txt", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} 1 2.txt", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"2.txt\"", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"2 3.txt\"", $"{windowsRoot}\\1\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} 2.txt", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} 2 3.txt", $"{windowsRoot}\\1\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"1\\2.txt\"", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} \"1 2\\3 4.txt\"", $"{windowsRoot}\\1 2\\3 4.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} 1\\2.txt", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{cmdArg} 1 2\\3 4.txt", $"{windowsRoot}\\1 2\\3 4.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"2\\3.txt\"", $"{windowsRoot}\\1\\2\\3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} \"2 3\\4 5.txt\"", $"{windowsRoot}\\1\\2 3\\4 5.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} 2\\3.txt", $"{windowsRoot}\\1\\2\\3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{cmdArg} 2 3\\4 5.txt", $"{windowsRoot}\\1\\2 3\\4 5.txt")
            };

            var failedScenarios = new List<string>();
            foreach (var testCase in testCases)
            {
                var testResult =
                    CommandLineUtility.GetAbsolutePathFromCommandLine(
                                                                          testCase.parentDirectory,
                                                                          testCase.argument
                                                                     );

                if (!testResult.Equals(testCase.result))
                {
                    failedScenarios.Add(
                        $"- Test case {testCases.IndexOf(testCase) + 1} failed with:\n" +
                        $"Parent Directory: {testCase.parentDirectory}\n" +
                        $"CommandLine Argument: {testCase.argument}\n" +
                        $"Test Case Output: {testResult}\n" +
                        $"Expected Output: {testCase.result}"
                    );
                }
            }

            if (failedScenarios.Count > 0)
            {
                throw new AssertFailedException(string.Join("\n\n\n", failedScenarios));
            }
        }

        [TestMethod]
        public void CommandLineTestForPowerShell()
        {
            var testCases = new List<CommandLineTestCase>
            {
                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"{windowsRoot}\\1.txt\"", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"{windowsRoot}\\1 2.txt\"", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} {windowsRoot}\\1.txt", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} {windowsRoot}\\1 2.txt", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"{windowsRoot}\\2.txt\"", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"{windowsRoot}\\2 3.txt\"", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} {windowsRoot}\\2.txt", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} {windowsRoot}\\2 3.txt", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"\\1.txt\"", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"\\1 2.txt\"", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \\1.txt", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \\1 2.txt", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"\\2.txt\"", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"\\2 3.txt\"", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \\2.txt", $"{windowsRoot}\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \\2 3.txt", $"{windowsRoot}\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"1.txt\"", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"1 2.txt\"", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} 1.txt", $"{windowsRoot}\\1.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} 1 2.txt", $"{windowsRoot}\\1 2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"2.txt\"", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"2 3.txt\"", $"{windowsRoot}\\1\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} 2.txt", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} 2 3.txt", $"{windowsRoot}\\1\\2 3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"1\\2.txt\"", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} \"1 2\\3 4.txt\"", $"{windowsRoot}\\1 2\\3 4.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} 1\\2.txt", $"{windowsRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{windowsRoot}\\", $"{pwshArg} 1 2\\3 4.txt", $"{windowsRoot}\\1 2\\3 4.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"2\\3.txt\"", $"{windowsRoot}\\1\\2\\3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} \"2 3\\4 5.txt\"", $"{windowsRoot}\\1\\2 3\\4 5.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} 2\\3.txt", $"{windowsRoot}\\1\\2\\3.txt"),

                new CommandLineTestCase($"{windowsRoot}\\1", $"{pwshArg} 2 3\\4 5.txt", $"{windowsRoot}\\1\\2 3\\4 5.txt")
            };

            var failedScenarios = new List<string>();
            foreach (var testCase in testCases)
            {
                var testResult =
                    CommandLineUtility.GetAbsolutePathFromCommandLine(
                                                                          testCase.parentDirectory,
                                                                          testCase.argument
                                                                     );

                if (!testResult.Equals(testCase.result))
                {
                    failedScenarios.Add(
                        $"- Test case {testCases.IndexOf(testCase) + 1} failed with:\n" +
                        $"Parent Directory: {testCase.parentDirectory}\n" +
                        $"CommandLine Argument: {testCase.argument}\n" +
                        $"Test Case Output: {testResult}\n" +
                        $"Expected Output: {testCase.result}"
                    );
                }
            }

            if (failedScenarios.Count > 0)
            {
                throw new AssertFailedException(string.Join("\n\n\n", failedScenarios));
            }
        }

        [TestMethod]
        public void CommandLineTestForWsl()
        {
            var testCases = new List<CommandLineTestCase>
            {
                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"{wslRoot}\\1.txt\"", $"{wslRoot}\\1.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"{wslRoot}\\1 2.txt\"", $"{wslRoot}\\1 2.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} {wslRoot}\\1.txt", $"{wslRoot}\\1.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} {wslRoot}\\1 2.txt", $"{wslRoot}\\1 2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"{wslRoot}\\2.txt\"", $"{wslRoot}\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"{wslRoot}\\2 3.txt\"", $"{wslRoot}\\2 3.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} {wslRoot}\\2.txt", $"{wslRoot}\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} {wslRoot}\\2 3.txt", $"{wslRoot}\\2 3.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"\\1.txt\"", $"{wslRoot}\\1.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"\\1 2.txt\"", $"{wslRoot}\\1 2.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \\1.txt", $"{wslRoot}\\1.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \\1 2.txt", $"{wslRoot}\\1 2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"\\2.txt\"", $"{wslRoot}\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"\\2 3.txt\"", $"{wslRoot}\\2 3.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \\2.txt", $"{wslRoot}\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \\2 3.txt", $"{wslRoot}\\2 3.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"1.txt\"", $"{wslRoot}\\1.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"1 2.txt\"", $"{wslRoot}\\1 2.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} 1.txt", $"{wslRoot}\\1.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} 1 2.txt", $"{wslRoot}\\1 2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"2.txt\"", $"{wslRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"2 3.txt\"", $"{wslRoot}\\1\\2 3.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} 2.txt", $"{wslRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} 2 3.txt", $"{wslRoot}\\1\\2 3.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"1\\2.txt\"", $"{wslRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} \"1 2\\3 4.txt\"", $"{wslRoot}\\1 2\\3 4.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} 1\\2.txt", $"{wslRoot}\\1\\2.txt"),

                new CommandLineTestCase($"{wslRoot}\\", $"{pwshArg} 1 2\\3 4.txt", $"{wslRoot}\\1 2\\3 4.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"2\\3.txt\"", $"{wslRoot}\\1\\2\\3.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} \"2 3\\4 5.txt\"", $"{wslRoot}\\1\\2 3\\4 5.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} 2\\3.txt", $"{wslRoot}\\1\\2\\3.txt"),

                new CommandLineTestCase($"{wslRoot}\\1", $"{pwshArg} 2 3\\4 5.txt", $"{wslRoot}\\1\\2 3\\4 5.txt"),

                new CommandLineTestCase($"{wslRoot}", $"{pwshArg} \"/etc/sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}", $"{pwshArg} \"etc/sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}", $"{pwshArg} /etc/sudo.conf", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}", $"{pwshArg} etc/sudo.conf", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}\\etc", $"{pwshArg} \"/etc/sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}\\etc", $"{pwshArg} \"sudo.conf\"", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}\\etc", $"{pwshArg} /etc/sudo.conf", $"{wslRoot}\\etc\\sudo.conf"),

                new CommandLineTestCase($"{wslRoot}\\etc", $"{pwshArg} sudo.conf", $"{wslRoot}\\etc\\sudo.conf")
            };

            var failedScenarios = new List<string>();
            foreach (var testCase in testCases)
            {
                var testResult =
                    CommandLineUtility.GetAbsolutePathFromCommandLine(
                                                                          testCase.parentDirectory,
                                                                          testCase.argument
                                                                     );

                if (!testResult.Equals(testCase.result))
                {
                    failedScenarios.Add(
                        $"- Test case {testCases.IndexOf(testCase) + 1} failed with:\n" +
                        $"Parent Directory: {testCase.parentDirectory}\n" +
                        $"CommandLine Argument: {testCase.argument}\n" +
                        $"Test Case Output: {testResult}\n" +
                        $"Expected Output: {testCase.result}"
                    );
                }
            }

            if (failedScenarios.Count > 0)
            {
                throw new AssertFailedException(string.Join("\n\n\n", failedScenarios));
            }
        }
    }
}