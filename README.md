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
    let crontab = cronexpr::parse_crontab("2 4 * * * Asia/Shanghai").unwrap();

    // case 1. find next timestamp with timezone
    assert_eq!(
        crontab
            .find_next("2024-09-24T10:06:52+08:00")
            .unwrap()
            .to_string(),
        "2024-09-25T04:02:00+08:00[Asia/Shanghai]"
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
            "2024-09-25T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-26T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-27T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-28T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-29T04:02:00+08:00[Asia/Shanghai]",
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
            "2024-09-25T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-26T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-27T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-28T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-29T04:02:00+08:00[Asia/Shanghai]",
            "2024-09-30T04:02:00+08:00[Asia/Shanghai]",
        ]
    );
}
```

## Usage

`cronexpr` is [on crates.io](https://crates.io/crates/cronexpr) and can be used by adding `cronexpr` to your dependencies in your project's `Cargo.toml`. Or more simply, just run `cargo add cronexpr`.

## Dependencies

`cronexpr` depends on:

* [thiserror](https://docs.rs/thiserror/) to define our `Error` type.
* [jiff](https://docs.rs/jiff/) for all the datetime things. This is almost internal, except:
  * the timestamp returned is a `jiff::Zoned`, although you can treat it as something defined by `cronexpr`.
  * the input type `MakeTimestamp` is a wrapper of `jiff::Timestamp`, but it's defined by `cronexpr` and enables you create a Timestamp from a string, milliseconds, nanoseconds, and more, without directly depend on `jiff::Timestamp` (you can still depend on it if you'd like).
* [winnow](https://docs.rs/winnow/) for parsing the crontab expression. This is fully internal: you don't need to understand it.

## Minimum Rust version policy

This crate is built against the latest stable release, and its minimum supported rustc version is 1.75.0.

The policy is that the minimum Rust version required to use this crate can be increased in minor version updates. For example, if cronexpr 1.0 requires Rust 1.20.0, then cronexpr 1.0.z for all values of z will also require Rust 1.20.0 or newer. However, cronexpr 1.y for y > 0 may require a newer minimum version of Rust.

## License

This project is licensed under [Apache License, Version 2.0](LICENSE).
