# picoview
Smol low-level windowing abstraction with a focus on audio plugin development 

> [!WARNING]
> This project is currently in development and the API is not final! Please do not rely on `picoview` in production.

## Goals

- Small API surface
- Minimal amount of dependencies 
- Complete OS abstraction
    - This includes abstracting away OS-dependent pixel scaling and only dealing with physical pixels. `picoview` provides a `WindowScale` event, which is treated as a hint more than anything else.

See [ROADMAP.md](ROADMAP.md) for more info.

## Prerequisites

### Linux

Install dependencies, e.g.:

```sh
sudo apt-get install libx11-dev libxcursor-dev libxrandr-dev libgl1-mesa-dev
```

## License

Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `picoview` by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
