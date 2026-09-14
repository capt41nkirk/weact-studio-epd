use core::convert::Infallible;
#[cfg(not(feature = "blocking"))]
use core::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};
use display_interface::{
    AsyncWriteOnlyDataCommand, DataFormat, DisplayError, WriteOnlyDataCommand,
};
use embedded_hal::{
    delay::DelayNs,
    digital::{ErrorType, InputPin, OutputPin},
};
use std::{cell::RefCell, rc::Rc};
use weact_studio_epd::{WeActStudio290BlackWhiteDriver, WeActStudio420BlackWhiteDriver};

#[derive(Debug, Clone, PartialEq)]
enum Event {
    Command(u8),
    Data(Vec<u8>),
    Delay(u32),
    Idle,
    Reset,
}
type Trace = Rc<RefCell<Vec<Event>>>;
struct Interface(Trace);
fn bytes(data: DataFormat<'_>) -> Vec<u8> {
    match data {
        DataFormat::U8(v) => v.to_vec(),
        DataFormat::U8Iter(v) => v.collect(),
        _ => panic!("unexpected format"),
    }
}
impl WriteOnlyDataCommand for Interface {
    fn send_commands(&mut self, data: DataFormat<'_>) -> Result<(), DisplayError> {
        for c in bytes(data) {
            self.0.borrow_mut().push(Event::Command(c));
        }
        Ok(())
    }
    fn send_data(&mut self, data: DataFormat<'_>) -> Result<(), DisplayError> {
        self.0.borrow_mut().push(Event::Data(bytes(data)));
        Ok(())
    }
}
impl AsyncWriteOnlyDataCommand for Interface {
    async fn send_commands(&mut self, data: DataFormat<'_>) -> Result<(), DisplayError> {
        WriteOnlyDataCommand::send_commands(self, data)
    }
    async fn send_data(&mut self, data: DataFormat<'_>) -> Result<(), DisplayError> {
        WriteOnlyDataCommand::send_data(self, data)
    }
}
struct Pin(Trace);
impl ErrorType for Pin {
    type Error = Infallible;
}
impl InputPin for Pin {
    fn is_high(&mut self) -> Result<bool, Infallible> {
        self.0.borrow_mut().push(Event::Idle);
        Ok(false)
    }
    fn is_low(&mut self) -> Result<bool, Infallible> {
        Ok(!self.is_high()?)
    }
}
impl OutputPin for Pin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        self.0.borrow_mut().push(Event::Reset);
        Ok(())
    }
    fn set_high(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
}
impl embedded_hal_async::digital::Wait for Pin {
    async fn wait_for_low(&mut self) -> Result<(), Infallible> {
        self.0.borrow_mut().push(Event::Idle);
        Ok(())
    }
    async fn wait_for_high(&mut self) -> Result<(), Infallible> {
        unreachable!()
    }
    async fn wait_for_rising_edge(&mut self) -> Result<(), Infallible> {
        unreachable!()
    }
    async fn wait_for_falling_edge(&mut self) -> Result<(), Infallible> {
        unreachable!()
    }
    async fn wait_for_any_edge(&mut self) -> Result<(), Infallible> {
        unreachable!()
    }
}
struct Delay(Trace);
impl DelayNs for Delay {
    fn delay_ns(&mut self, ns: u32) {
        self.0.borrow_mut().push(Event::Delay(ns));
    }
}
impl embedded_hal_async::delay::DelayNs for Delay {
    async fn delay_ns(&mut self, ns: u32) {
        DelayNs::delay_ns(self, ns);
    }
}

// These mocks resolve immediately. No executor or runtime is needed.
#[cfg(not(feature = "blocking"))]
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("mock unexpectedly yielded"),
    }
}
#[cfg(feature = "blocking")]
fn run<T>(value: T) -> T {
    value
}
fn driver() -> (
    WeActStudio420BlackWhiteDriver<Interface, Pin, Pin, Delay>,
    Trace,
) {
    let trace = Trace::default();
    let driver = WeActStudio420BlackWhiteDriver::new(
        Interface(trace.clone()),
        Pin(trace.clone()),
        Pin(trace.clone()),
        Delay(trace.clone()),
    );
    (driver, trace)
}
fn commands(trace: &Trace) -> Vec<u8> {
    trace
        .borrow()
        .iter()
        .filter_map(|e| {
            if let Event::Command(c) = e {
                Some(*c)
            } else {
                None
            }
        })
        .collect()
}
fn writes(trace: &Trace, command: u8) -> Vec<Vec<u8>> {
    trace
        .borrow()
        .windows(2)
        .filter_map(|w| match w {
            [Event::Command(c), Event::Data(d)] if *c == command => Some(d.clone()),
            _ => None,
        })
        .collect()
}
fn assert_busy_margin(trace: &Trace) {
    let events = trace.borrow();
    for (i, event) in events.iter().enumerate() {
        if *event == Event::Command(0x20) {
            assert_eq!(events[i + 1], Event::Delay(1_000_000));
            assert_eq!(events[i + 2], Event::Idle);
        }
    }
}

#[test]
fn ssd1683_full_fast_partial_full_transitions_and_ram_sync() {
    let (mut d, t) = driver();
    run(d.init()).unwrap();
    assert_eq!(writes(&t, 0x3c), vec![vec![0x01]]);
    let old = [0xff; 15000];
    run(d.full_update_from_buffer(&old)).unwrap();
    t.borrow_mut().clear();
    let new = [0x55; 15000];
    run(d.fast_update_from_buffer(&new)).unwrap();
    assert!(!commands(&t).contains(&0x32), "SSD1683 must use OTP LUT");
    assert_eq!(writes(&t, 0x21), vec![vec![0, 0]]);
    assert_eq!(writes(&t, 0x22), vec![vec![0xfc]]);
    assert_eq!(writes(&t, 0x24), vec![new.to_vec(), new.to_vec()]);
    assert_eq!(writes(&t, 0x26), vec![new.to_vec()]);
    let c = commands(&t);
    assert!(
        c.iter().position(|v| *v == 0x20).unwrap() < c.iter().position(|v| *v == 0x26).unwrap()
    );
    assert_busy_margin(&t);
    t.borrow_mut().clear();
    run(d.fast_partial_update_from_buffer(&[0xa5; 4], 392, 296, 8, 4)).unwrap();
    assert_eq!(writes(&t, 0x44), vec![vec![49, 49]; 3]);
    assert_eq!(writes(&t, 0x45), vec![vec![0x28, 1, 0x2b, 1]; 3]);
    assert_eq!(writes(&t, 0x26), vec![vec![0xa5; 4]]);
    t.borrow_mut().clear();
    run(d.full_update_from_buffer(&old)).unwrap();
    run(d.fast_update_from_buffer(&new)).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0xf7], vec![0xfc]]);
    assert!(!commands(&t).contains(&0x32));
    assert_busy_margin(&t);
}

#[test]
fn first_fast_request_only_performs_one_full_refresh() {
    let (mut d, t) = driver();
    run(d.init()).unwrap();
    t.borrow_mut().clear();
    run(d.fast_update_from_buffer(&[0x81; 15000])).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0xf7]]);
    assert_eq!(commands(&t).iter().filter(|c| **c == 0x20).count(), 1);
    assert_eq!(writes(&t, 0x26), vec![vec![0x81; 15000]; 2]);
}

#[test]
fn low_level_first_fast_refresh_does_not_follow_full_with_partial() {
    let (mut d, t) = driver();
    run(d.init()).unwrap();
    run(d.write_bw_buffer(&[0xff; 15000])).unwrap();
    run(d.write_red_buffer(&[0xff; 15000])).unwrap();
    t.borrow_mut().clear();
    run(d.fast_refresh()).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0xf7]]);
    assert_busy_margin(&t);
}

#[test]
fn first_partial_initializes_white_baseline_and_accepts_one_row() {
    let (mut d, t) = driver();
    run(d.init()).unwrap();
    t.borrow_mut().clear();
    run(d.fast_partial_update_from_buffer(&[0x00], 392, 299, 8, 1)).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0xf7]]);
    assert_eq!(writes(&t, 0x24), vec![vec![0xff; 15000], vec![0], vec![0]]);
    assert_eq!(writes(&t, 0x26), vec![vec![0xff; 15000], vec![0], vec![0]]);
    // Full refresh must restore the full RAM window after the partial write.
    let windows = writes(&t, 0x44);
    assert_eq!(windows[4], vec![0, 49]);
}

#[test]
fn sleep_wake_keeps_baseline_but_explicit_init_requires_full_refresh() {
    let (mut d, t) = driver();
    run(d.init()).unwrap();
    run(d.full_update_from_buffer(&[0xff; 15000])).unwrap();
    t.borrow_mut().clear();
    run(d.sleep()).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0x83]]);
    assert_eq!(writes(&t, 0x10), vec![vec![1]]);
    assert_busy_margin(&t);
    run(d.wake_up()).unwrap();
    assert!(commands(&t).contains(&0x12));
    t.borrow_mut().clear();
    run(d.fast_update_from_buffer(&[0x55; 15000])).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0xfc]]);
    run(d.init()).unwrap();
    t.borrow_mut().clear();
    run(d.fast_update_from_buffer(&[0xff; 15000])).unwrap();
    assert_eq!(writes(&t, 0x22), vec![vec![0xf7]]);
}

#[test]
fn smaller_panel_still_loads_its_partial_lut() {
    let t = Trace::default();
    let mut d = WeActStudio290BlackWhiteDriver::new(
        Interface(t.clone()),
        Pin(t.clone()),
        Pin(t.clone()),
        Delay(t.clone()),
    );
    run(d.init()).unwrap();
    run(d.full_update_from_buffer(&[0xff; 4736])).unwrap();
    t.borrow_mut().clear();
    run(d.fast_update_from_buffer(&[0x55; 4736])).unwrap();
    assert_eq!(writes(&t, 0x32).len(), 1);
    run(d.fast_update_from_buffer(&[0xff; 4736])).unwrap();
    assert_eq!(writes(&t, 0x32).len(), 1);
}

#[test]
fn recreated_driver_can_explicitly_resume_retained_panel() {
    let (mut original, _) = driver();
    run(original.init()).unwrap();
    run(original.full_update_from_buffer(&[0xff; 15000])).unwrap();
    run(original.sleep()).unwrap();
    drop(original);
    let (mut resumed, trace) = driver();
    run(resumed.resume_retained()).unwrap();
    trace.borrow_mut().clear();
    run(resumed.fast_update_from_buffer(&[0x55; 15000])).unwrap();
    assert_eq!(writes(&trace, 0x22), vec![vec![0xfc]]);
    assert_eq!(writes(&trace, 0x24), vec![vec![0x55; 15000]; 2]);
    assert_eq!(writes(&trace, 0x26), vec![vec![0x55; 15000]]);
    run(resumed.init()).unwrap();
    trace.borrow_mut().clear();
    run(resumed.fast_update_from_buffer(&[0xff; 15000])).unwrap();
    assert_eq!(writes(&trace, 0x22), vec![vec![0xf7]]);
}
