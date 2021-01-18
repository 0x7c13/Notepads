# Notepads CI/CD documentation

* after merging the PR, the first run of the main workflow will not complete successfully, because it requires specific setup explained in this documentation

## *. Set up Dependabot

Dependabot is a GitHub native security tool that goes through the dependencies in your project and creates alerts, and PRs with updates when a new and/or non-vulnerable version is found.

- for PRs with version updates, this pipeline comes pre-configured for all current dependency sources in your project, so at "Insights" tab -> "Dependency graph" -> "Dependabot", you should be able to see all tracked sources of dependencies, when they have been checked last and view a full log of the last check

![Dependabot_tab](/ScreenShots/CI-CD_DOCUMENTATION/Dependabot_tab.png)

![Dependabot_log_page](/ScreenShots/CI-CD_DOCUMENTATION/Dependabot_log_page.png)

### Set up security alerts and updates
##### - GitHub, through Dependabot, also natively offers a security check for vulnerable dependencies

1. Go to "Settings" tab of your repo

2. Go to "Security&Analysis" section

3. Click "Enable" for both "Dependabot alerts" and "Dependabot security updates"

- By enabling "Dependabot alerts", you would be notified for any vulnerable dependencies in your project. At "Security" tab -> "Dependabot alerts", you can manage all alerts. By clicking on an alert, you would be able to see a detailed explanation of the vulnerability and a viable solution.

![Dependabot_alerts_page](/ScreenShots/CI-CD_DOCUMENTATION/Dependabot_alerts_page.png)

![Dependabot_alert_page](/ScreenShots/CI-CD_DOCUMENTATION/Dependabot_alert_page.png)

- By enabling "Dependabot security updates", you authorize Dependabot to create PRs specifically for **security updates**

![Dependabot_PRs](/ScreenShots/CI-CD_DOCUMENTATION/Dependabot_PRs.png)

### Set up Dependency graph
##### - The "Dependency graph" option should be enabled by default for all public repos, but in case it isn't:

1. Go to "Settings" tab of your repo

2. Go to "Security&Analysis" section

3. Click "Enable" for the "Dependency graph" option

- this option enables the "Insights" tab -> "Dependency graph" section -> "Dependencies" tab, in which all the dependencies for the project are listed, under the different manifests they are included in

![Dependabot_dependency_graph](/ScreenShots/CI-CD_DOCUMENTATION/Dependabot_dependency_graph.png)

NOTE: **screenshots are only exemplary**

<br>

## *. CodeQL

CodeQL is GitHub's own industry-leading semantic code analysis engine. CodeQL requires no setup, because it comes fully pre-configured by us. 

To activate it and see its results, only a push commit or a merge of a PR to the default branch of your repository, is required. 

We've also configured CodeQL to run on schedule, so every day at 8:00AM UTC, it automatically test the code.

- you can see the results here at **Security** tab -> **Code scanning alerts** -> **CodeQL**:

![CodeQL_results](/ScreenShots/CI-CD_DOCUMENTATION/CodeQL_results.png)

- on the page of each result, you can see an explanation of what the problem is and also one or more solutions:

![CodeQL_alert_page](/ScreenShots/CI-CD_DOCUMENTATION/CodeQL_alert_page.png)

NOTE: **screenshots are only exemplary**

#

Built with ❤ by [Pipeline Foundation](http://pipeline.foundation)