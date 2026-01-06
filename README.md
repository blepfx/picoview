# picoview
Smol low-level windowing abstraction with a focus on audio plugin development 

> [!WARNING]
> This project is currently in development and the API is not final! Please do not rely on `picoview` in production. MacOS backend is still broken!

## Goals

- Small API surface
    - `picoview` should be easy to use and provide only the essentials for window creation, event handling and OS abstraction.
- Low compile times
    - `picoview` should compile fast and not bloat your compile times. Uses minimum amount of dependencies.
- Complete OS abstraction
    - `picoview` should behave the same on all supported platforms (Windows, macOS, Linux).
    - This includes abstracting away OS-dependent pixel scaling and only dealing with physical pixels. `picoview` provides a `WindowScale` event, which is treated as a hint more than anything else.
- Audio plugin focused
    - `picoview` should be suitable for audio plugin development. This means it should be possible to hook into an existing event loop provided by a plugin host.

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
