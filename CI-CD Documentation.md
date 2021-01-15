# Bogus CI/CD documentation

* after merging the PR, the first run of the main workflow will not complete successfully, because it requires specific setup explained in this documentation


## 1. Set up SonarCloud 
  
SonarCloud is a cloud-based code quality and security service.

1. Go to https://sonarcloud.io/

2. Click the "Log in" button and create a new account or connect with GitHub account (recommended).

3. At the top right corner click the "+" sign.

4. From the dropdown select "Create new Organization".

5. Click the button "Choose an organization on Github".

6. Select an account for your organization setup.

7. On **Repository Access** select "Only select repositories" and select your project and click the "Save" button.

8. On the "Create organization page" don't change your **Key** and click "Continue".

9. Select the Free plan then click the "Create Organization" button to finalize the creation of your Organization.

10. From the dropdown select "Analyze new project".

11. Select the "Bogus" project and click "Set Up" button at the top right corner.

12. Under the "Choose another analysis method" sign click the "With Github Actions" sign. 

13. Copy the Name of the token and the Value and use them on step "16".

14. To Create a secret on GitHub click the fast forward button **Settings>Secrets** .
 
15. Then click "New Repository secret"

16. Enter the "Name" and the "Value" and click **Add Secret**.

17. No further steps are required for this setup.

18. Run manually your workflow one time to deliver the code to SonarCloud.

19. In order to set a "Quality gate" follow the next steps.

19. After the run go to the Project page.

20. Click on the button "Set new code definition" and select  "Previous version".

21. Manually run the workflow and there you have set a Quality gate.