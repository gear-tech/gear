/**
 * Javascript module for skipping CI
 */

const core = require('@actions/core');
const github = require('@actions/github');

const CHECKS = ["check", "build"]
const [owner, repo] = ["gear-tech", "gear"];

async function mock() {
  const head_sha = core.getInput("head-sha")
  for (check of CHECKS) {
    const { data: res } = await github.rest.checks.create({
      owner,
      repo,
      name: `${check} / linux`,
      head_sha: HEAD_SHA,
      status: "completed",
      conclusion: "success",
    });

    core.info(`Created check ${check}`);
    core.info(JSON.stringify(res, null, 2));
  }
}

mock().catch(err => {
  core.error("ERROR: ", err.message);
  try {
    console.log(JSON.stringify(err, null, 2))
  } catch (e) {
    // Ignore JSON errors for now.
  }

  console.log(e.stack)
})
