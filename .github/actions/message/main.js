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
  const token = core.getInput("token");
  const octokit = github.getOctokit(token);
  for (check of CHECKS) {
    const { data: res } = await octokit.rest.checks.create({
      owner,
      repo,
      name: `${check} / linux`,
      head_sha,
      status: "completed",
      conclusion: "success",
    });

    core.info(`Created check "${check} / linux"`);
    core.info(res.html_url);
  }
}

/**
 * Main function.
 */
async function main() {
  const {
    pull_request: { title, head: { sha, ref: branch }, labels: _labels },
    repository: { full_name: fullName }
  } = github.context.payload;
  const labels = _labels.map(l => l.name);
  const message = ps.execSync(`git log --format=%B -n 1 ${sha}`, { encoding: "utf-8" }).trim();

  console.log("message: ", message);
  console.log("head-sha: ", sha);
  console.log("title: ", title);
  console.log("full name: ", fullName);
  console.log("labels: ", labels);
  console.log("--------------------")

  // Calculate configurations.
  const isDepbot = fullName === `${owner}/${repo}` && title.includes(DEPBOT);
  const skipCache = [title, message].some(s => s.includes(SKIP_CACHE));
  const skipCI = [title, message].some(s => s.includes(SKIP_CI));
  const build = !skipCI && (isDepbot || BUILD_LABELS.some(label => labels.includes(label)));
  const macos = !skipCI && labels.includes(MACOS);
  const cache = SCCACHE_PREFIX + branch.replace("/", "_");

  // Set outputs
  core.setOutput("build", build);
  core.setOutput("check", !skipCI);
  core.setOutput("macos", macos)
  if (!skipCache) {
    core.setOutput("cache", cache);
    console.log("cache: ", cache);
  }

  console.log("check: ", !skipCI);
  console.log("build: ", build);
  console.log("macos: ", macos);

  // Mock checks if skipping CI.
  if (skipCI) await mock(sha);
}

main().catch(err => {
  core.error("ERROR: ", err.message);
  core.error(err.stack)
})
