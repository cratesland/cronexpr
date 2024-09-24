# Crontab Expression Parser and Driver

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MSRV 1.75][msrv-badge]](https://www.whatrustisit.com)
[![Apache 2.0 licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/cronexpr.svg
[crates-url]: https://crates.io/crates/cronexpr
[docs-badge]: https://docs.rs/cronexpr/badge.svg
[msrv-badge]: https://img.shields.io/badge/MSRV-1.75-green?logo=rust
[docs-url]: https://docs.rs/cronexpr
[license-badge]: https://img.shields.io/crates/l/cronexpr
[license-url]: LICENSE
[actions-badge]: https://github.com/tisonkun/cronexpr/workflows/CI/badge.svg
[actions-url]:https://github.com/tisonkun/cronexpr/actions?query=workflow%3ACI

## Overview

A library to parse and drive the crontab expression.

## Documentation

* [API documentation on docs.rs](https://docs.rs/cronexpr)

## Example

Here is a quick example that shows how to parse a cron expression and drive it with a timestamp:

```rust
fn main() {
    let crontab = cronexpr::parse_crontab("2 4 * * * Asia/ Shanghai").unwrap();

    // case 1. find next timestamp with timezone
    assert_eq!(
        crontab
            .find_next("2024-09-24T10:06:52+08:00")
            .unwrap()
            .to_string(),
        "2024-09-25T04:02:00+08:00[Asia/ Shanghai]"
    );

    // case 2. iter over next timestamps without upper bound
    let driver = crontab
        .drive("2024-09-24T10:06:52+08:00", None::<cronexpr::MakeTimestamp>)
        .unwrap();
    assert_eq!(
        driver
            .take(5)
            .map(|ts| ts.map(|ts| ts.to_string()))
            .collect::<Result<Vec<_>, cronexpr::Error>>()
            .unwrap(),
        vec![
            "2024-09-25T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-26T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-27T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-28T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-29T04:02:00+08:00[Asia/ Shanghai]",
        ]
    );

    // case 3. iter over next timestamps with upper bound
    let driver = crontab
        .drive(
            "2024-09-24T10:06:52+08:00",
            Some("2024-10-01T00:00:00+08:00"),
        )
        .unwrap();
    assert_eq!(
        driver
            .map(|ts| ts.map(|ts| ts.to_string()))
            .collect::<Result<Vec<_>, cronexpr::Error>>()
            .unwrap(),
        vec![
            "2024-09-25T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-26T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-27T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-28T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-29T04:02:00+08:00[Asia/ Shanghai]",
            "2024-09-30T04:02:00+08:00[Asia/ Shanghai]",
        ]
    );
}
```

## Usage

`cronexpr` is [on crates.io](https://crates.io/crates/cronexpr) and can be used by adding `cronexpr` to your dependencies in your project's `Cargo.toml`. Or more simply, just run `cargo add cronexpr`.



```shell
cargo add cronexpr
```
