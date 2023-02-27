# GSDK

[![CI][ci1]][ci2]
[![docs][docs1]][docs2]

[ci1]: https://github.com/gear-tech/gear/workflows/CI/badge.svg
[ci2]: https://github.com/gear-tech/gear/actions/workflows/CI.yaml
[docs1]: https://img.shields.io/badge/current-docs-brightgreen.svg
[docs2]: https://docs.gear.rs/gsdk/index.html

Rust SDK for gear network.


## Example

```rust
use gsdk::signer::Signer;

#[tokio::main]
async fn main() {
    // Connect to "wss://rpc-node.gear-tech.io:443" by default.
    let signer = Api::new(None).signer("//Alice", None);

    // Transaction with block details.
    let tx = signer.transfer("//Bob", 42).await.expect("Transfer value failed.");

    // Fetch all of the events associated with this transaction.
    for events in tx.fetch_events().await {
        // ...
    }
}
```


## License

GPL v3.0 with a classpath linking exception
