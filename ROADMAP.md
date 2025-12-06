## Roadmap


| Feature                                               | Windows  | MacOS    | X11      | Priority |
| ----------------------------------------------------- | -------- | -------- | -------- | -------- |
| Window creation                                       | :ok:     |          |          |          |
|  - Top-level decorated windows                        | :ok:     | :ok:     | :ok:     | High     |
|  - Top-level undecorated windows                      | :ok:     | :x:      | :ok:     | Medium   |
|  - Embedded windows                                   | :ok:     | :x:      | :ok:     | High     |
|  - Parented windows                                   | :x:      | :x:      | :x:      | Low      |
| Window events                                         |          |          |          |          |
|  - `MouseUp`                                          | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseDown`                                        | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseMove`                                        | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseScroll`                                      | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyUp`                                            | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyDown`                                          | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyModifiers`                                     | :ok:     | :ok:     | :ok:     | High     |
|  - `WindowFocus`                                      | :ok:     | :x:      | :ok:     | Low      |
|  - `WindowScale`                                      | :ok:     | :ok:     | :ok:[^1] | Medium   |
|  - `WindowMove`                                       | :ok:     | :x:      | :ok:     | Low      |
|  - `WindowResize`                                     | :ok:     | :x:      | :ok:     | High     |
|  - `WindowFrame`                                      | :ok:     | :ok:     | :ok:     | High     |
|  - `WindowDamage`                                     | :ok:     | :x:      | :ok:     | Medium   |
|  - `DragHover`                                        | :x:      | :x:      | :x:      | Low      |
|  - `DragAccept`                                       | :x:      | :x:      | :x:      | Low      |
|  - `DragCancel`                                       | :x:      | :x:      | :x:      | Low      |
| Event loop wakeup                                     | :ok:     | :x:      | :ok:     | High     |
| Vertical blank synchronization                        | :ok:[^2] | :ok:     | :o:[^3]  | High     |
| OpenGL context creation                               | :ok:     | :x:      | :ok:     | High     |
| Clipboard text get/set                                | :ok:     | :ok:     | :x:      | Medium   |
| Pixel scaling abstraction                             | :ok:     | :x:      | :ok:     | High     |
| Set position                                          | :ok:     | :x:      | :ok:     | Medium   |
| Set size                                              | :ok:     | :x:      | :ok:     | High     |
| User resizable                                        | :ok:     | :x:      | :ok:     | High     |
| Set title                                             | :ok:     | :ok:     | :ok:     | Medium   |
| Set visibility                                        | :ok:     | :ok:     | :ok:     | Medium   |
| Close window                                          | :ok:     | :x:      | :ok:     | High     |
| Capture keyboard events[^4]                           | :ok:     | :ok:     | :ok:     | High     |
| Open browser/explorer                                 | :ok:     | :ok:     | :ok:     | Medium   |
| Cursor icons                                          | :ok:     | :ok:     | :ok:     | Medium   |
| Cursor warping                                        | :ok:     | :ok:     | :ok:[^5] | Medium   |

[^1]: Only a single global scaling factor is supported (no per-monitor scaling)
[^2]: It is possible to use the DXGI api for lower latency [?] (we only use DWMFlush for now)
[^3]: XPresent seems unreliable; we fallback to doing manual frame events with poll timeout (synced to XRandR provided refresh rates)
[^4]: Some DAWs consume key events meant for plugins, keyboard capturing is meant to avoid that when needed
[^5]: Broken on XWayland, seems to be a Wayland limitation?
