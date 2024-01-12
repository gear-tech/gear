# Skip CI Action

There is a bad behavior in github that once we set the required actions
( `build / linux`, `check / linux` in [gear-tech/gear][gear] ), we can
not skip them on insubstantial pull requests as well, besides, it will
leave the ugly yellow dot on the CI status.

In case of solving this problem, we implemented an action script to mock
the required actions in case we do want to skip them.

This skipping action is triggered by label `[skip-ci]` in commit message,
different from the [`[skip ci]` from github][github].

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

In the meanwhile, once this skip-check is triggered, it will create two
checks in the present pull request:

- `build / linux`
- `check / linux`

## LICENSE

GPL-3.0-only

[gear]: https://github.com/gear-tech/gear
[github]: https://docs.github.com/en/actions/managing-workflow-runs/skipping-workflow-runs
