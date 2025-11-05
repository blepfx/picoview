## Roadmap


| Feature                                               | Windows  | MacOS    | X11      | Priority |
| ----------------------------------------------------- | -------- | -------- | -------- | -------- |
| Window creation                                       | :ok:     |          |          |          |
|  - Top-level decorated windows                        | :ok:     | :ok:     | :ok:     | High     |
|  - Top-level undecorated windows                      | :ok:     | :x:      | :ok:     | Medium   |
|  - Embedded windows                                   | :ok:     | :x:      | :ok:     | High     |
|  - Parented windows                                   | :ok:     | :x:      | :x:      | Medium   |
| Window events                                         |          |          |          |          |
|  - `MouseUp`                                          | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseDown`                                        | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseMove`                                        | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseScroll`                                      | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyUp`                                            | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyDown`                                          | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyModifiers`                                     | :ok:     | :ok:     | :ok:     | High     |
|  - `WindowFocus`                                      | :ok:     | :x:      | :ok:     | Low      |
|  - `WindowScale`                                      | :ok:     | :x:      | :ok:[^1] | Medium   |
|  - `WindowMove`                                       | :ok:     | :x:      | :ok:     | Low      |
|  - `WindowResize`                                     | :x:      | :x:      | :x:      | High     |
|  - `WindowFrame`                                      | :ok:[^2] | :ok:[^3] | :ok:[^4] | High     |
|  - `WindowInvalidate`                                 | :x:      | :x:      | :ok:     | High     |
|  - `DragHover`                                        | :x:      | :x:      | :x:      | Low      |
|  - `DragAccept`                                       | :x:      | :x:      | :x:      | Low      |
|  - `DragCancel`                                       | :x:      | :x:      | :x:      | Low      |
| OpenGL context creation                               | :ok:     | :x:      | :ok:     | High     |
| Clipboard text get/set                                | :ok:     | :ok:     | :x:      | Medium   |
| Pixel scaling abstraction                             | :ok:     | :x:      | :ok:     | High     |
| Alpha transparency                                    | :x:      | :x:      | :x:      | Low      |
| Set position                                          | :ok:[^5] | :x:      | :ok:     | Medium   |
| Set size                                              | :ok:     | :x:      | :ok:     | High     |
| User resizable[^6]                                    | :x:      | :x:      | :x:      | High     |
| Set title                                             | :ok:     | :x:      | :ok:     | Medium   |
| Set visibility                                        | :ok:     | :x:      | :ok:     | Medium   |
| Close window                                          | :ok:     | :x:      | :ok:     | High     |
| Grab keyboard[^7]                                     | :ok:     | :x:      | :ok:     | High     |
| Open browser/explorer                                 | :ok:     | :ok:     | :ok:     | Medium   |
| Cursor icons                                          | :ok:     | :ok:     | :ok:     | Medium   |
| Cursor warping                                        | :ok:     | :ok:     | :ok:[^8] | Medium   |
| Cursor hit passthrough[^9]                            | :x:      | :x:      | :x:      | Low      |
| Drag & Drop accept[^10]                               | :x:      | :x:      | :x:      | Low      |

[^1]: Only a single global scaling factor is supported (no per-monitor scaling)
[^2]: Currently only DWM waiting is supported, ideally we would have to do per-monitor DXGI wait.
[^3]: Currently only main monitor sync is supported
[^4]: Currently broken on XWayland, so it fallbacks to a fixed 60hz timer (use XRandR to get screen refresh rate?)
[^5]: Initial (`None`) position is broken (should be centered), children position is broken (should be parent-relative).
[^6]: No API for that yet
[^7]: Some DAWS tend to consume key events meant for plugins, keyboard hooking/grabbing is meant to avoid that when needed
[^8]: Broken on XWayland, seems to be a Wayland limitation?
[^9]: No API for that yet
[^10]: No API for that yet
