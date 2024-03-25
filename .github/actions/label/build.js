/**
 * Javascript module for the label action.
 */

const [owner, repo] = ["gear-tech", "gear"];
const { LABEL, REF, HEAD_SHA, TITLE, NUMBER, REPO } = process.env;
const linux =
  LABEL === "A0-pleasereview" ||
  LABEL === "A4-insubstantial" ||
  LABEL === "A2-mergeoncegreen";
const checks = linux ? ["linux (debug)", "win-cross (debug)"] : ["x86"];
const workflow_id = linux
  ? ".github/workflows/build.yml"
  : ".github/workflows/build-macos.yml";

/**
 *  Sleep for ms milliseconds.
 **/
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 *  If skipping this action.
 **/
const checkSkip = async ({ github, core }) => {
  core.info(`Checking if need to skip dispatch from ${REPO}:${REF}`);
  if (REPO === "gear-tech/gear" && REF.startsWith("dependabot")) {
    core.info("This pull request is from dependabot, skipping dispatching.");
    process.exit(0);
  }

  const {
    data: { check_runs },
  } = await github.rest.checks.listForRef({
    owner,
    repo,
    ref: REF,
  });

  if (
    check_runs.filter(
      (run) =>
        (run.name === "build" && run.conclusion !== "skipped") ||
        run.name === "build / linux (debug)" ||
        run.name === "build / macos / x86"
    ).length > 0
  ) {
    core.info(
      "Build has already been processed, check runs: ",
      JSON.stringify(check_runs, null, 2)
    );

    process.exit(0);
  }
};

/**
 *  Create build checks.
 *
 *  TODO:
 *    * Queue the new created checks to check suite PR (#3087).
 *    * Support re-runing the checks. (#3088)
 **/
const createChecks = async ({ core, github }) => {
  let status = {};
  for (const check of checks) {
    const { data: res } = await github.rest.checks.create({
      owner,
      repo,
      name: `build / ${check}`,
      head_sha: HEAD_SHA,
    });

    core.info(`Created check ${check}`);
    status[check] = res;
  }

  return status;
};

/**
 *  Dispatch the target workflow.
 */
const dispatchWorkflow = async ({ core, github }) => {
  await github.rest.actions.createWorkflowDispatch({
    owner,
    repo,
    workflow_id,
    ref: REF,
    inputs: {
      title: TITLE,
      number: NUMBER,
    },
  });

  // Wait for the workflow to be dispatched.
  await sleep(10000);

  // Get the target workflow run
  const {
    data: { workflow_runs },
  } = await github.rest.actions.listWorkflowRuns({
    owner,
    repo,
    workflow_id,
    head_sha: HEAD_SHA,
  });

  if (workflow_runs.length === 0) {
    core.setFailed(`Incorrect workflow runs`);
    return;
  }

  let sorted_runs = workflow_runs.sort((a, b) => {
    return new Date(b.created_at) - new Date(a.created_at);
  });

  return sorted_runs[0];
};

/// List jobs of workflow run.
const listJobs = async ({ github, core, run_id }) => {
  const {
    data: { jobs },
  } = await github.rest.actions.listJobsForWorkflowRun({
    owner,
    repo,
    run_id,
  });

  if (jobs.length === 0) {
    core.setFailed(`Empty jobs from dispatched workflow`);
    return;
  }

  // DEBUG
  console.log(
    "All jobs:",
    jobs.map((job) => job.name)
  );
  console.log("All checks", checks);
  // DEBUG

  const requiredJobs = jobs.filter((job) => checks.includes(job.name));
  if (requiredJobs.length !== checks.length) {
    core.setFailed(`Incorrect count for disptached jobs`);
    return;
  }

  return requiredJobs;
};

/**
 *  The main function.
 **/
module.exports = async ({ github, core }) => {
  // await checkSkip({ github, core });

  const run = await dispatchWorkflow({ core, github });
  core.info(`Dispatched workflow ${run.html_url}`);
  let labelChecks = await createChecks({ core, github });

  // Wait for the jobs to be completed.
  while (true) {
    const jobs = await listJobs({ github, core, run_id: run.id });
    completed = jobs.filter((job) => job.status === "completed").length;

    for (const job of jobs) {
      let checkJob = labelChecks[job.name];
      if (
        checkJob.status !== job.status ||
        checkJob.conclusion !== job.conclusion
      ) {
        core.info(
          `Updating check ${job.name}, status: ${job.status}, conclusion: ${job.conclusion}`
        );

        let { status, conclusion } = job;

        let data = {
          owner,
          repo,
          check_run_id: checkJob.id,
          status,
          output: {
            title: `Build ${job.name}`,
            summary: `ref ${job.html_url}`,
          },
        };

        labelChecks[job.name].status = status;
        if (conclusion) {
          data.conclusion = conclusion;
          labelChecks[job.name].conclusion = conclusion;
        }

        await github.rest.checks.update(data);
      } else {
        continue;
      }
    }

    if (completed === checks.length) {
      core.info("All jobs completed.");
      return;
    } else {
      await sleep(10000);
    }
  }
};
