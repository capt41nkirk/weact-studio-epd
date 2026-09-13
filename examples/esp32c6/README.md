# ESP32-C6 / WeAct 4.2-inch B/W timing example

This example targets the GDEY042T81 / SSD1683 panel. It uses ESP HAL 1.1.0,
a current ESP-IDF application descriptor, and the driver's blocking API.

## Wiring for the tested setup

| Display connection | ESP32-C6 GPIO |
| --- | ---: |
| MOSI / SDA | 9 |
| SCLK / SCL | 1 |
| CS | 18 |
| DC | 21 |
| RESET | 22 |
| BUSY (active high) | 23 |
| Switched display ground | 14, held **low** |

GPIO14 is specific to this setup. The example holds it low for the entire run,
including display sleep. If your display uses a fixed ground connection, adapt
that line to your board. Display power must remain connected to retain the RAM
baseline across `sleep()` / `wake_up()`.

SPI is mode 0 at **100 kHz**, matching the working weather-station setup.
Higher rates tested on this wiring did not reliably activate the display.
Do not interpret a 1–2 ms refresh return as success: the example checks for
that failure and panics. BUSY must actually assert and then clear.

## Build and flash

From this directory:

```powershell
cargo build --release --locked
cargo run --release --locked -- --port COM4
```

Adjust COM4 to the board's port (`espflash list-ports`). The toolchain file
installs the RISC-V target. The application descriptor is required by newer
bootloaders; do not bypass the descriptor check.

If the board stays at `wait usb download`, press RESET without holding BOOT
and reconnect the monitor if Windows re-enumerates USB. During testing,
Espressif's Python esptool reset worked when the espflash reset did not:

```powershell
python -m esptool --chip esp32c6 --port COM4 chip-id
espflash monitor --port COM4 --non-interactive --no-reset
```

The Python command requires esptool in that Python environment. Close other
serial monitors before flashing or resetting.

## What the test does

- Draws an initial full frame.
- Performs three full-frame differential updates, timing refresh separately
  from RAM transfers.
- Performs ten 320x64 region updates at (40, 160), alternating black/white
  backgrounds deliberately to exercise both pixel transition directions.
- Sleeps and wakes the display halfway through those updates.
- Performs a recovery full update followed by another partial update.
- Sleeps the panel and prints `TIMING TEST COMPLETE`.

The static text should remain intact throughout the partial updates. The final
screen reads `Full frame fast 2`, `Static text must stay`, and `Partial update 9`.
The program then idles; press RESET to repeat it.

## Hardware results

Tested 2026-09-13 on the connected ESP32-C6 and WeAct 4.2-inch B/W display,
with GPIO14 low and SPI at 100 kHz. Visible partial updates and preservation
of the static line were confirmed by the user.

| Operation | Measured time |
| --- | ---: |
| Initial full update, including RAM transfers | 8754 ms |
| Full-frame fast refresh, refresh only | 964 ms |
| Full-frame fast update, including RAM synchronization | 4597 ms |
| 320x64 partial update, including RAM synchronization | 1588 ms |

See [timing-results.txt](timing-results.txt) for the final serial capture.
The full-frame update includes three 15,000-byte transfers, which account
for approximately 3.6 seconds at 100 kHz. Region updates reduce this overhead.
These are measurements of this setup, not guaranteed limits at all temperatures.

## Driver sequence

The SSD1683 uses its built-in waveform, following the
[GxEPD2 GDEY042T81 driver](https://github.com/ZinggJM/GxEPD2/blob/master/src/gdey/GxEPD2_420_GDEY042T81.cpp).
The final Rust implementation uses `0x21: 00 00`, `0x22: FC`, then `0x20`
for partial refresh. It waits 1 ms for BUSY to assert before waiting for idle,
and synchronizes previous/current image RAM after refreshing. Full refresh
keeps the normal temperature-sensor sequence (`0x22: F7`).
