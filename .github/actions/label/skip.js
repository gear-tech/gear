/**
 * Javascript module for skipping CI
 */

const SKIP_CI = "[skip-ci]";
const { TITLE, HEAD_SHA } = process.env;
const CHECKS = ["linux", "win-cross"]
const [owner, repo] = ["gear-tech", "gear"];

module.exports = async ({ github, core }) => {
  if (!TITLE.includes(SKIP_CI)) return;

  core.info(`Skipping CI for ${TITLE}`);

  for (check of CHECKS) {
    const { data: res } = await github.rest.checks.create({
      owner,
      repo,
      name: `build / ${check}`,
      head_sha: HEAD_SHA,
      status: "completed",
      conclusion: "success",
    });

    core.info(`Created check ${check}`);
    core.info(JSON.stringify(res, null, 2));
  }
}
