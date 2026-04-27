## Roadmap


| Feature                                               | Windows  | MacOS    | X11      | Priority |
| ----------------------------------------------------- | -------- | -------- | -------- | -------- |
| Window creation                                       | :ok:     |          |          |          |
|  - Top-level decorated windows                        | :ok:     | :ok:     | :ok:     | High     |
|  - Top-level undecorated windows                      | :ok:     | :x:      | :ok:     | Medium   |
|  - Embedded parented windows                          | :ok:     | :ok:     | :ok:     | High     |
|  - Transient parented windows                         | :ok:     | :ok:     | :ok:     | Low      |
| Window events                                         |          |          |          |          |
|  - `MouseUp`                                          | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseDown`                                        | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseMove`                                        | :ok:     | :ok:     | :ok:     | High     |
|  - `MouseScroll`                                      | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyUp`                                            | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyDown`                                          | :ok:     | :ok:     | :ok:     | High     |
|  - `KeyText`                                          | :x:      | :x:      | :x:      | Medium   |
|  - `KeyModifiers`                                     | :ok:     | :ok:     | :ok:     | High     |
|  - `WindowClose`                                      | :ok:     | :ok:     | :ok:     | Low      |
|  - `WindowFocus`                                      | :ok:     | :ok:     | :ok:     | Low      |
|  - `WindowScale`                                      | :ok:     | :ok:     | :ok:[^1] | Medium   |
|  - `WindowMove`                                       | :ok:     | :x:      | :ok:     | Low      |
|  - `WindowResize`                                     | :ok:     | :ok:     | :ok:     | High     |
|  - `WindowFrame`                                      | :ok:     | :ok:     | :ok:     | High     |
|  - `WindowDamage`                                     | :ok:     | :ok:     | :ok:     | Medium   |
| Clipboard                                             |          |          |          |          |
|  - Copy Text                                          | :ok:     | :ok:     | :ok:     | High     |
|  - Paste Text                                         | :ok:     | :ok:     | :ok:     | High     |
|  - Copy Files                                         | :ok:     | :x:      | :ok:     | Medium   |
|  - Paste Files                                        | :ok:     | :x:      | :ok:     | Medium   |
| Drag&Drop                                             |          |          |          |          |
|  - Text                                               | :x:      | :ok:     | :x:      | Low      |
|  - Files                                              | :ok:     | :ok:     | :x:      | Medium   |
|  - Enter/Leave/Hover events                           | :x:      | :ok:     | :x:      | Low      |
| Event loop wakeup                                     | :ok:     | :ok:     | :ok:     | High     |
| Vertical blank synchronization                        | :ok:[^2] | :ok:     | :o:[^3]  | High     |
| OpenGL context creation                               | :ok:     | :ok:     | :ok:     | High     |
| Pixel scaling abstraction                             | :ok:     | :x:      | :ok:     | High     |
| Set position                                          | :ok:     | :x:      | :ok:     | Medium   |
| Set size                                              | :ok:     | :ok:     | :ok:     | High     |
| User resizable                                        | :ok:     | :ok:     | :ok:     | High     |
| Set title                                             | :ok:     | :ok:     | :ok:     | Medium   |
| Set visibility                                        | :ok:     | :ok:     | :ok:     | Medium   |
| Close window                                          | :ok:     | :ok:     | :ok:     | High     |
| Capture keyboard events[^4]                           | :ok:     | :ok:     | :ok:     | High     |
| Open browser/explorer                                 | :ok:     | :ok:     | :ok:     | Medium   |
| Cursor icons                                          | :ok:     | :ok:     | :ok:     | Medium   |
| Cursor warping                                        | :ok:     | :ok:     | :ok:[^5] | Medium   |

[^1]: Only a single global scaling factor is supported (no per-monitor scaling)
[^2]: It is possible to use the DXGI api for lower latency [?] (we only use DWMFlush for now)
[^3]: XPresent seems unreliable; we fallback to doing manual frame events with poll timeout (synced to XRandR provided refresh rates)
[^4]: Some DAWs consume key events meant for plugins, keyboard capturing is meant to avoid that when needed
[^5]: Broken on XWayland, seems to be a Wayland limitation?

## Known issues
- MacOS:
    - Window/event/size positioning is all over the place due to differences in coordinate systems.
    - Needed an API to create a `Size` from os-dependent units for proper interoperation with VST3/CLAP APIs
- Windows:
    - Some cursor icons are not supported, fallbacks used.