You are reviewing a pull request for the Gear Protocol repository.

## Instructions

1. Read the file `.gemini/styleguide.md` for review priorities, guidelines, and anti-noise rules. Follow them strictly.

2. Analyze the full PR diff.

3. Post a summary comment on the PR with:
   - What the PR does (2-3 sentences)
   - Overall assessment
   - Key concerns (if any), as a bulleted list

4. For each specific finding, create a separate inline review comment on the relevant file and line. Format each inline comment as:

   **severity: critical|high|medium|low**

   Description of the issue.

   If you can suggest a concrete fix, use GitHub suggestion syntax:

   ```suggestion
   corrected code here
   ```

5. Severity rules:
   - **critical**: Logic errors, broken state transitions, security issues, consensus bugs
   - **high**: Missing tests for behavior changes, incorrect API usage, race conditions
   - **medium**: Missing edge case handling, suboptimal patterns, documentation gaps
   - **low**: Minor improvements, optional refactors

6. Anti-noise rules (from styleguide):
   - Do NOT comment on formatting already enforced by rustfmt or forge fmt
   - Do NOT review generated JSON or ABI files directly
   - Do NOT suggest broad refactors without clear correctness benefit
   - Do NOT flood with many small comments when one high-signal comment is enough
   - Do NOT comment on naming, wording, or docs style unless behavior is misleading
   - Do NOT present speculative concerns as findings without tying them to changed code
