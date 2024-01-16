/**
 * Javascript module for status check.
 */

const core = require('@actions/core');
const github = require('@actions/github');

const BUILD_LABELS = ["A0-pleasereview", 'A4-insubstantial', 'A2-mergeoncegreen'];
const CHECKS = ["check", "build"]
const DEPBOT = "[depbot]";
const MACOS = "E2-forcemacos";
const SKIP_CI = "[skip-ci]";
const SKIP_CACHE = "[skip-cache]";
const [owner, repo] = ["gear-tech", "gear"];

/**
 * Get labels from issue.
 *
 * NOTE: api - https://docs.github.com/en/rest/issues/labels?apiVersion=2022-11-28#list-labels-for-an-issue
 * ---
 *
 * @param { number } issue_number
 * @returns { string[] }
 */
async function getLabels(issue_number) {
  const { data: labels } = await github.rest.issues.listLabelsOnIssue({
    owner,
    repo,
    issue_number,
  });

  return labels.map(label => label.name)
}

/**
 * Mock required checks
 *
 * NOTE: api - https://docs.github.com/en/rest/checks/runs?apiVersion=2022-11-28#create-a-check-run
 * ---
 *
 * @param {string} head_sha
 * @returns {Promise<void>}
 */
async function mock(head_sha) {
  for (check of CHECKS) {
    const { data: res } = await github.rest.checks.create({
      owner,
      repo,
      name: `${check} / linux`,
      head_sha,
      status: "completed",
      conclusion: "success",
    });

    core.info(`Created check ${check}`);
    core.info(JSON.stringify(res, null, 2));
  }
}

/**
 * Main function.
 */
async function main() {
  const message = core.getInput("message");
  const sha = core.getInput("head-sha");
  const issue = github.context.payload.number;
  const title = github.context.payload.title;
  const fullName = github.context.payload.repository.full_name;

  core.info("message: ", message);
  core.info("head-sha: ", sha);
  core.info("title: ", title);
  core.info("issue: ", number);
  core.info("full name: ", fullName);

  console.log("payload: ", JSON.stringify(github.context.payload, null, 2));

  // Calculate configurations.
  const labels = getLabels(issue);
  const isDepbot = fullName === `${owner}/${repo}` && title.includes(DEPBOT);
  const skipCache = [title, message].some(s => s.includes(SKIP_CACHE));
  const skipCI = [title, message].some(s => s.includes(SKIP_CI));
  const build = !skipCI && (isDepbot || BUILD_LABELS.some(label => labels.includes(label)));

  // Set outputs
  core.setOutput("build", build);
  core.setOutput("cache", !skipCache);
  core.setOutput("check", !skipCI);
  core.setOutput("macos", labels.includes(MACOS))

  // Mock checks if skipping CI.
  if (skipCI) await mock(sha);
}

main().catch(err => {
  core.error("ERROR: ", err.message);
  try {
    console.log(JSON.stringify(err, null, 2))
  } catch (e) {
    // Ignore JSON errors for now.
  }

  console.log(e.stack)
})
