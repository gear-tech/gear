# Github Mock Action

There is a bad behavior in github that once we set the required actions
( `build / linux`, `check / linux` in [gear-tech/gear][gear] ), we can
not skip them on insubstantial pull requests as well, besides, it will
leave the ugly yellow dot on the CI status.

In case of solving this problem, we implemented an action script to mock
the required actions in case we do want to skip them.

## Usage

```yaml
steps:
  - uses: ./.github/actions/mock
```

In the meanwhile, once this skip-check is triggered, it will create two
checks in the present pull request:

- `build / linux`
- `check / linux`

## LICENSE

GPL-3.0-only

[gear]: https://github.com/gear-tech/gear
