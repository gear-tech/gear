# Github Message Action

This actions calculates configuration for the workflow of the pull requests
in [gear-tech/gear][gear].

```yaml
outputs:
  build:
    description: "If trigger step build."
  cache:
    description: "If enable cache."
  check:
    description: "If trigger step check."
  macos:
    description: "If enable macos."
```

### Skip CI

This action has introduced a label `[skip-ci]` which could be embedded
in the pull request title or the commit message.

The label `[skip-ci]` is different from the github labels, see
[skipping-workflow-runs][github] for more details.

### Mock Required Checks

There is a bad behavior in github that once we set the required actions
( `build / linux`, `check / linux` in [gear-tech/gear][gear] ), we can
not skip them on insubstantial pull requests as well, besides, it will
leave the ugly yellow dot on the CI status.

For solving this problem, we have implemented an action script to mock
the required actions in case we do want to skip them:

When `[skip-ci]` is found, this action will create two checks in the present
pull request:

- `build / linux`
- `check / linux`

## Usage

```yaml
steps:
  - uses: ./.github/actions/message
    with:
      token: ${{ secrets.GITHUB_TOKEN }}
```

## LICENSE

GPL-3.0-only

[gear]: https://github.com/gear-tech/gear
[github]: https://docs.github.com/en/actions/managing-workflow-runs/skipping-workflow-runs
