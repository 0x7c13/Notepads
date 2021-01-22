# Notepads CI/CD documentation

* after merging the PR, the first run of the main workflow will not complete successfully, because it requires specific setup explained in this documentation

## *. Run workflow manually

Once you've set up all the steps above correctly, you should be able to successfully complete a manual execution of the main workflow "Notepads CI/CD Pipeline".

  1. Go to GitHub project -> "Actions" tab

  2. From the "Workflows" list on the left, click on "Notepads CI/CD Pipeline"

  3. On the right, next to the "This workflow has a workflow_dispatch event trigger" label, click on the "Run workflow" dropdown, make sure the default branch is selected (if not manually changed, should be main or master) in the "Use workflow from" dropdown and click the "Run workflow" button

![Actions_workflow_dispatch](/ScreenShots/CI-CD_DOCUMENTATION/Actions_workflow_dispatch.png)

  4. Once the workflow run has completed successfully, move on to the next step of the documentation

NOTE: **screenshots are only exemplary**

<br>

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

## *. How to create a release

The "Release sequence" encompasses creating a release in your GitHub repo and adding the necessary files to it.

Files added are:
- Deployment package (msixbundle)
- Source code (zip)
- Source code (tar.gz)

Note: **not every commit to your master branch creates a release**

Follow these instructions for any commit (push or PR merge) to your master branch, you would like to create a release.

Whenever you would like to trigger the "Create release" sequence, you would need one of three keywords at the start of your commit title. Each of the three keywords corresponds to a number in your release version i.e. v1.2.3. The release versioning uses the ["Conventional Commits" specification](https://www.conventionalcommits.org/en/v1.0.0/):

* "fix: ..." - this keyword corresponds to the last number v1.2.**3**, also known as PATCH;
* "feat: ..." - this keyword corresponds to the middle number v1.**2**.3, also known as MINOR;
* "perf: ..." - this keyword corresponds to the first number v**1**.2.3, also known as MAJOR. In addition, to trigger a MAJOR release, you would need to write "BREAKING CHANGE: ..." in the description of the commit, with an empty line above it to indicate it is in the "**footer**" portion of the description;

Note: when making a MAJOR release by committing through a terminal, use the multiple line syntax to add the commit title on one line and then adding an empty line, and then adding the "BREAKING CHANGE: " label

### EXAMPLES

Example(fix/PATCH): <br>
`
git commit -a -m "fix: this is a PATCH release triggering commit"
`
<br>
`
git push origin master
`
<br>
Result: v1.2.3 -> **v1.2.4**
<br>
<br>
<br>
Example(feat/MINOR): <br>
`
git commit -a -m "feat: this is a MINOR release triggering commit"
`
<br>
`
git push origin master
`
<br>
Result: v1.2.3 -> **v1.3.0**
<br>
<br>
<br>
Example(perf/MAJOR): <br>
``
git commit -a -m "perf: this is a MAJOR release triggering commit `
``
<br> 
&gt;&gt; <br> 
&gt;&gt; ` BREAKING CHANGE: this is the breaking change"
`
<br>
`
git push origin master
`
<br>
Result: v1.2.3 -> **v2.0.0**
<br>
<br>
Note: in the MAJOR release example, the PowerShell multiline syntax ` (backtick) is used. After writing a backtick, a press of the Enter key should open a new line.

#

Built with ❤ by [Pipeline Foundation](http://pipeline.foundation)