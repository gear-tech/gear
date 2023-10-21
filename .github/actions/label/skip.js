/**
 * Javascript module for skipping CI
 */

const SKIP_CI_LABEL = "[skip-ci]";
const { TITLE, HEAD_SHA, SKIP_CI } = process.env;
const CHECKS = ["check", "build"]
const [owner, repo] = ["gear-tech", "gear"];

module.exports = async ({ github, core }) => {
  if (!TITLE.includes(SKIP_CI_LABEL) && SKIP_CI != 1) return;

  core.info(`Skipping CI for ${TITLE}`);

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
