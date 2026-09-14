# weact-studio-epd

Unofficial Rust driver for WeAct Studio e-paper displays.

The driver exposes both write access to the screen frame buffers and higher-level `embedded-graphics` support.

By default this driver uses `async`. If you prefer to use a blocking API instead you can enable the `blocking` feature.

## Supported displays

| Display | Colors | Supported | Partial update[^1] | Fast refresh[^2] | Tested |
|---|---|:---:|:---:|:---:|:---:|
| WeAct 1.54 inch 200x200 B/W | Black, White | ✕ |  |  |  |
| WeAct 2.13 inch 122x250 B/W | Black, White | ✓ | ✓ | ✓ | ✓ |
| WeAct 2.13 inch 122x250 B/W/R | Black, White, Red | ✓ |  | ✕ |  |
| WeAct 2.9 inch 128x296 B/W | Black, White | ✓ | ✓ | ✓ | ✓ |
| WeAct 2.9 inch 128x296 B/W/R | Black, White, Red | ✓ |  | ✕ | ✓ |
| WeAct 4.2 inch 400x300 B/W | Black, White | ✓ | ✓ | ✓ | ✓ |

[^1]: Allows updating part of the screen buffer to save IO time and potentially memory.

[^2]: Refresh the screen without flickering the screen a few times.

## Examples

See the `examples` folder for complete usage examples.

## 4.2-inch black/white fast updates

The 400x300 GDEY042T81 (SSD1683) uses its built-in waveform for differential
updates. The driver does not upload the smaller panels' custom LUT to it.
A full refresh uses update control `0xF7`; a fast refresh uses `0xFC`.

For a blocking driver, use:

```rust,ignore
driver.init()?;
driver.full_update(&display)?; // establishes the initial image
// Draw the next image into display.
driver.fast_update(&display)?;
// Or update only a byte-aligned region using a smaller DisplayBlackWhite:
driver.fast_partial_update(&region, 40, 160)?;
```

With the default async API, await these calls. A first `fast_update` falls back
to one full refresh. A first partial update initializes the rest of the screen
to white and performs a full refresh. Later updates use the fast waveform.
The partial region's x coordinate and width must be multiples of 8; its height
may be a single row.

The high-level update methods synchronize both controller RAM planes after each
refresh. If using `write_bw_buffer` and `fast_refresh` directly, write the new
image to both the B/W and red (previous-image) RAM after the refresh completes.
Do not overwrite the previous-image RAM before a differential refresh.

`sleep()` powers down the panel's driving voltages and enters RAM-retaining
sleep. `wake_up()` restores its registers so the same driver can continue fast
updates. After removing panel power, initialize and establish a full image again.
A recreated 4.2-inch B/W driver can call `resume_retained()` instead of `init()`
only if the application guarantees continuous panel power, a successfully
synchronized baseline followed by `sleep()`, and an uninterrupted MCU deep-sleep
cycle. This explicit opt-in restores the same baseline state as `wake_up()`;
ordinary `new()` / `init()` still require a full image. Applications should
invalidate their retained marker before starting an update and commit it only
after RAM synchronization and panel sleep succeed. Use an occasional full
update to clear accumulated ghosting.

See [the ESP32-C6 timing example](examples/esp32c6/README.md) for wiring,
flashing, and measured results.

## Features

- `blocking`: Replaces the API with a blocking version. This disables the `async` API so you cannot use both in the same project.
- `graphics`: Enables `embedded-graphics` support. Enabled by default.

## Credits

This driver is based on the following crates:

- [`epd-waveshare`](https://crates.io/crates/epd-waveshare)
- [`ssd1680`](https://crates.io/crates/ssd1680)

## License

This crate is licenced under:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
