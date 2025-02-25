#![no_std]
#![no_main]

use core::fmt::Write;
use display_interface_spi::SPIInterface;
use embedded_graphics::{
    geometry::Point,
    mono_font::MonoTextStyle,
    text::{Alignment, Text, TextStyle, TextStyleBuilder},
    Drawable,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl,
    delay::Delay,
    gpio::{Input, Io, Level, Output, NO_PIN},
    peripherals::Peripherals,
    prelude::*,
    spi::{master::Spi, SpiMode},
    system::SystemControl,
};
use heapless::String;
use profont::PROFONT_24_POINT;
use weact_studio_epd::{graphics::DisplayRotation, WeActStudio420BlackWhiteDriver};
use weact_studio_epd::{
    graphics::{buffer_len, Display420BlackWhite, DisplayBlackWhite},
    Color,
};

#[entry]
fn main() -> ! {
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    let clocks = ClockControl::max(system.clock_control).freeze();
    let delay = Delay::new(&clocks);

    esp_println::logger::init_logger_from_env();

    log::info!("Intializing SPI Bus...");

    let sclk = io.pins.gpio6; //scl on 4.2 display
    let mosi = io.pins.gpio7; //sda
    let cs = io.pins.gpio15;
    let dc = io.pins.gpio21;
    let rst = io.pins.gpio22;
    let busy = io.pins.gpio23;

    let spi_bus = Spi::new(peripherals.SPI2, 100.kHz(), SpiMode::Mode0, &clocks).with_pins(
        Some(sclk),
        Some(mosi),
        NO_PIN,
        NO_PIN, // cs is handled by the exclusive device
    );

    // Convert pins into InputPins and OutputPins
    /*
        CS: OutputPin,
        BUSY: InputPin,
        DC: OutputPin,
        RST: OutputPin,
    */
    let cs = Output::new(cs, Level::High);
    let busy = Input::new(busy, esp_hal::gpio::Pull::Up);
    let dc = Output::new(dc, Level::Low);
    let rst = Output::new(rst, Level::High);

    log::info!("Intializing SPI Device...");
    let spi_device = ExclusiveDevice::new(spi_bus, cs, delay).expect("SPI device initialize error");
    let spi_interface = SPIInterface::new(spi_device, dc);

    // Setup EPD
    log::info!("Intializing EPD...");
    let mut driver = WeActStudio420BlackWhiteDriver::new(spi_interface, busy, rst, delay);
    let mut display = Display420BlackWhite::new();
    display.set_rotation(DisplayRotation::Rotate0);
    driver.init().unwrap();

    let mut partial_display =
        DisplayBlackWhite::<400, 128, { buffer_len::<Color>(400, 128) }>::new();
    partial_display.set_rotation(DisplayRotation::Rotate0);

    let style = MonoTextStyle::new(&PROFONT_24_POINT, Color::Black);
    let _ = Text::with_text_style(
        "Hello World!",
        Point::new(8, 68),
        style,
        TextStyle::default(),
    )
    .draw(&mut display);

    driver.full_update(&display).unwrap();

    log::info!("Sleeping for 5s...");
    driver.sleep().unwrap();
    delay.delay(1_000.millis());

    let mut n: u8 = 0;
    loop {
        log::info!("Wake up!");
        partial_display.clear(Color::White);

        let mut string_buf = String::<30>::new();
        write!(string_buf, "Update {}!", n).unwrap();
        let _ = Text::with_text_style(
            &string_buf,
            Point::new(160, 32),
            style,
            TextStyleBuilder::new().alignment(Alignment::Right).build(),
        )
        .draw(&mut partial_display)
        .unwrap();
        string_buf.clear();

        driver.wake_up().unwrap();
        driver
            .fast_partial_update(&partial_display, 0, 156)
            .unwrap();

        log::info!("Sleeping for 5s...");
        driver.sleep().unwrap();
        let timesleep = 3_000; // + (n as u64 * 60_000);
        delay.delay(timesleep.millis());

        n = n.wrapping_add(1); // Wrap from 0..255
    }
}
