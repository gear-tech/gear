/**
 * Javascript module for status check.
 */

const ps = require("child_process");
const core = require("@actions/core");
const github = require("@actions/github");

const BUILD_LABELS = [
  "A0-pleasereview",
  "A4-insubstantial",
  "A2-mergeoncegreen",
];
const CHECKS = ["check / linux", "build / linux (debug)"];
const DEPBOT = "[depbot]";
const WINDOWS_NATIVE = "E1-forcenatwin";
const MACOS = "E2-forcemacos";
const RELEASE = "E3-forcerelease";
const PRODUCTION = "E4-forceproduction";
const SKIP_CI = "[skip-ci]";
const VALIDATOR_LABEL = "check-validator";
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
  for (const name of CHECKS) {
    const { data: res } = await octokit.rest.checks.create({
      owner,
      repo,
      name,
      head_sha,
      status: "completed",
      conclusion: "success",
    });

    core.info(`Created check "${name}"`);
    core.info(res.html_url);
  }
}

/**
 * Main function.
 */
async function main() {
  const {
    pull_request: {
      title,
      head: { sha },
      labels: _labels,
    },
    repository: { full_name: fullName },
  } = github.context.payload;
  const labels = _labels.map((l) => l.name);
  const message = ps
    .execSync(`git log --format=%B -n 1 ${sha}`, { encoding: "utf-8" })
    .trim();

  console.log("message: ", message);
  console.log("head-sha: ", sha);
  console.log("title: ", title);
  console.log("full name: ", fullName);
  console.log("labels: ", labels);

  // Calculate configurations.
  const isDepbot = fullName === `${owner}/${repo}` && title.includes(DEPBOT);
  const skipCI = [title, message].some((s) => s.includes(SKIP_CI));
  const build =
    !skipCI &&
    (isDepbot || BUILD_LABELS.some((label) => labels.includes(label)));
  const validator = !skipCI && labels.includes(VALIDATOR_LABEL);
  const win_native = !skipCI && labels.includes(WINDOWS_NATIVE);
  const macos = !skipCI && labels.includes(MACOS);
  const release = !skipCI && labels.includes(RELEASE);
  const production = !skipCI && labels.includes(PRODUCTION);

  // Set outputs
  core.setOutput("build", build);
  core.setOutput("check", !skipCI);
  core.setOutput("win-native", win_native);
  core.setOutput("macos", macos);
  core.setOutput("release", release);
  core.setOutput("validator", validator);
  core.setOutput("production", production);

  console.log("---");
  console.log("build: ", build);
  console.log("check: ", !skipCI);
  console.log("native windows: ", win_native);
  console.log("macos: ", macos);
  console.log("production: ", production);
  console.log("validator: ", validator);
  console.log("release: ", release);

  // Mock checks if skipping CI.
  if (skipCI) await mock(sha);
}

main().catch((err) => {
  core.error("ERROR: ", err.message);
  core.error(err.stack);
});
