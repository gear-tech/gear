/**
 * Javascript module for status check.
 */

const ps = require("child_process");
const core = require('@actions/core');
const github = require('@actions/github');

const BUILD_LABELS = ["A0-pleasereview", 'A4-insubstantial', 'A2-mergeoncegreen'];
const CHECKS = ["check", "build"]
const DEPBOT = "[depbot]";
const MACOS = "E2-forcemacos";
const SCCACHE_PREFIX = '/mnt/sccache/';
const SKIP_CI = "[skip-ci]";
const SKIP_CACHE = "[skip-cache]";
const [owner, repo] = ["gear-tech", "gear"];

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
  console.log(github.context.payload);
  console.log(JSON.stringify(github.context.payload, null, 2));
  const {
    pull_request: { head: { sha }, labels: _labels, title },
    repository: { full_name: fullName }
  } = github.context.payload;
  const labels = _labels.map(l => l.name);
  const message = ps.execSync(`git log --format=%B -n 1 ${sha}`, { encoding: "utf-8" }).trim();

  core.info("message: ", message);
  core.info("head-sha: ", sha);
  core.info("title: ", title);
  core.info("full name: ", fullName);
  core.info("labels: ", labels);

  // Calculate configurations.
  const isDepbot = fullName === `${owner}/${repo}` && title.includes(DEPBOT);
  const skipCache = [title, message].some(s => s.includes(SKIP_CACHE));
  const skipCI = [title, message].some(s => s.includes(SKIP_CI));
  const build = !skipCI && (isDepbot || BUILD_LABELS.some(label => labels.includes(label)));
  const branch = github.context.payload.repository.ref;

  // Set outputs
  core.setOutput("build", build);
  core.setOutput("check", !skipCI);
  core.setOutput("macos", labels.includes(MACOS))
  if (!skipCache) core.setOutput("cache", `${SCCACHE_PREFIX}/${branch.replace("/", "_")}`);

  // Mock checks if skipping CI.
  if (skipCI) await mock(sha);
}

main().catch(err => {
  core.error("ERROR: ", err.message);
  core.error(err.stack)
})
