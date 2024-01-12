# Message Checker Action

This action is for resolve labels `[skip-ci]` and `[skip-cache]` from
the current commit message, it will [mock the required checks][mock]
if `[skip-ci]` is found in the commit message.

The label `[skip-ci]` is different from the github labels, see
[skipping-workflow-runs][github] for more details.

## Usage

There are two outputs in this action:

```yaml
outputs:
  skip-ci:
    description: "If label [skip-ci] is found in the commit message."
  skip-cache:
    description: "If label [skip-cache] is found in the commit message."
```

For example in github action:

```yaml
steps:
  - name: Check Skipping
    uses: .github/actions/skip/action.yml
    id: check-skip

  - name: My step refers to `[skip-ci]`
    if: check-skip.outputs.skip-ci == '1'
    run: echo "label [skip-ci] found in the commit message!"

  - name: My step refers to `[skip-cache]`
    if: check-skip.outputs.skip-cache == '1'
    run: echo "label [skip-cache] found in the commit message!""
```

## LICENSE

GPL-3.0-only

[gear]: https://github.com/gear-tech/gear
[github]: https://docs.github.com/en/actions/managing-workflow-runs/skipping-workflow-runs
[mock]: ../mock/README.md
