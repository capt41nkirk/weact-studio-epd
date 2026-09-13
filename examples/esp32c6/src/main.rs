#![no_std]
#![no_main]

use core::fmt::Write;
use display_interface_spi::SPIInterface;
use embedded_graphics::{geometry::Point, mono_font::MonoTextStyle, text::Text, Drawable};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    spi::master::{Config as SpiConfig, Spi},
    time::{Instant, Rate},
};
use heapless::String;
use profont::PROFONT_24_POINT;
use weact_studio_epd::{
    graphics::{buffer_len, Display420BlackWhite, DisplayBlackWhite},
    Color, WeActStudio420BlackWhiteDriver,
};

esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("PANIC: {}", info);
    loop {}
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    // GPIO14 is the display ground connection in this setup; keep it low.
    let _display_power = Output::new(peripherals.GPIO14, Level::Low, OutputConfig::default());
    let delay = Delay::new();
    esp_println::logger::init_logger_from_env();
    // Give the USB serial monitor time to attach after reset.
    delay.delay_millis(2_000);
    log::info!("GDEY042T81 timing test: SPI 100 kHz, BUSY GPIO23 active high");

    let spi_bus = Spi::new(
        peripherals.SPI2,
        SpiConfig::default().with_frequency(Rate::from_khz(100)),
    )
    .unwrap()
    .with_sck(peripherals.GPIO1)
    .with_mosi(peripherals.GPIO9);
    let cs = Output::new(peripherals.GPIO18, Level::High, OutputConfig::default());
    let busy = Input::new(
        peripherals.GPIO23,
        InputConfig::default().with_pull(Pull::Up),
    );
    let dc = Output::new(peripherals.GPIO21, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO22, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new(spi_bus, cs, delay).unwrap();
    let mut driver =
        WeActStudio420BlackWhiteDriver::new(SPIInterface::new(spi_device, dc), busy, rst, delay);
    driver.init().unwrap();
    let mut display = Display420BlackWhite::new();
    let style = MonoTextStyle::new(&PROFONT_24_POINT, Color::Black);
    Text::new("4.2 inch fast refresh", Point::new(8, 40), style)
        .draw(&mut display)
        .unwrap();
    Text::new("Static text must stay", Point::new(8, 90), style)
        .draw(&mut display)
        .unwrap();

    let started = Instant::now();
    driver.full_update(&display).unwrap();
    log::info!(
        "initial full update: {} ms",
        (Instant::now() - started).as_millis()
    );
    delay.delay_millis(1_000);

    // Measure the refresh separately from all three RAM transfers.
    for n in 0..3 {
        let mut label = String::<32>::new();
        write!(label, "Full frame fast {}", n).unwrap();
        display.clear(Color::White);
        Text::new("Static text must stay", Point::new(8, 90), style)
            .draw(&mut display)
            .unwrap();
        Text::new(&label, Point::new(8, 40), style)
            .draw(&mut display)
            .unwrap();
        let started = Instant::now();
        driver.write_bw_buffer(display.buffer()).unwrap();
        let refresh_started = Instant::now();
        driver.fast_refresh().unwrap();
        let refresh_ms = (Instant::now() - refresh_started).as_millis();
        driver.write_red_buffer(display.buffer()).unwrap();
        driver.write_bw_buffer(display.buffer()).unwrap();
        let total_ms = (Instant::now() - started).as_millis();
        log::info!(
            "full frame fast {}: refresh={} ms, total={} ms",
            n,
            refresh_ms,
            total_ms
        );
        assert!(refresh_ms >= 10, "BUSY never asserted: check wiring");
        assert!(total_ms < 5_000, "fast update exceeded 5 seconds");
        delay.delay_millis(1_000);
    }

    let mut partial = DisplayBlackWhite::<320, 64, { buffer_len::<Color>(320, 64) }>::new();
    for n in 0..10 {
        if n == 5 {
            log::info!("Testing display sleep / wake (RAM retained)");
            driver.sleep().unwrap();
            delay.delay_millis(1_000);
            driver.wake_up().unwrap();
        }
        // Alternating polarity tests both erase and draw transitions.
        let (background, foreground) = if n % 2 == 0 {
            (Color::Black, Color::White)
        } else {
            (Color::White, Color::Black)
        };
        partial.clear(background);
        let mut label = String::<32>::new();
        write!(label, "Partial update {}", n).unwrap();
        Text::new(
            &label,
            Point::new(8, 40),
            MonoTextStyle::new(&PROFONT_24_POINT, foreground),
        )
        .draw(&mut partial)
        .unwrap();
        let started = Instant::now();
        driver.fast_partial_update(&partial, 40, 160).unwrap();
        let total_ms = (Instant::now() - started).as_millis();
        log::info!("partial {}: total={} ms", n, total_ms);
        assert!(
            total_ms >= 10 && total_ms < 5_000,
            "unexpected partial update timing"
        );
        delay.delay_millis(1_000);
    }

    let started = Instant::now();
    driver.full_update(&display).unwrap();
    log::info!(
        "recovery full update: {} ms",
        (Instant::now() - started).as_millis()
    );
    let started = Instant::now();
    driver.fast_partial_update(&partial, 40, 160).unwrap();
    log::info!(
        "partial after full: {} ms",
        (Instant::now() - started).as_millis()
    );
    driver.sleep().unwrap();
    log::info!("TIMING TEST COMPLETE: final screen Full frame fast 2 / Static text must stay / Partial update 9");
    loop {
        delay.delay_millis(1_000);
    }
}
