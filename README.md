# diurn-mic-rs

Parser, model, registry, and diff for the ISO 10383 Market Identifier Code
registry. Publishes as the crate [`diurn-mic`](https://crates.io/crates/diurn-mic).

No async runtime, no HTTP client, no CLI framework — this crate reads a CSV and
gives you typed records. Rendering and fetching are somebody else's job.

```rust
let outcome = MicRegistry::load_csv(reader, LoadOptions { published })?;
let nyse = outcome.registry.get(mic!("XNYS"));
```

A malformed row never fails the load. Every problem the loader finds comes back
as a structured `Issue` alongside the data, because ISO will eventually ship a
bad record and that must degrade rather than break.

## Data source and attribution

MIC data is published by the ISO 10383 Registration Authority at
<https://www.iso20022.org/market-identifier-codes>, which operates the registry
free of charge. This crate parses that file; it does not redistribute it as a
download. Test fixtures pin a dated vintage for reproducibility.

The registry is published on the second Monday of each month, and the changes it
carries take effect on the fourth Monday — so a freshly published file contains
records that are not yet in force. See `Status::Updated` and `as_of`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
