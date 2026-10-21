//! RGBadge: 6x7 RGB LED matrix, one 7-pin charlieplex per colour, and one
//! button (BUT) that turns the badge off and on.

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use cortex_m::peripheral::SCB;
use cortex_m_rt::{exception, ExceptionFrame};
use defmt::*;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant};

use py32_badgekit::button::Button;
use py32_badgekit::charlieplex::{self, Led};
use py32_badgekit::power::{self, WakePin};
use py32_badgekit::{font, Port};
use py32_hal::gpio::{Input, Pull};
use py32_hal::rcc::Sysclk;
use py32_hal::time::Hertz;
use {defmt_rtt as _, panic_halt as _};

// ---- Board -----------------------------------------------------------------

const SYSCLK_HZ: u32 = 24_000_000;

/// 42 slots * 100us = 4.2ms -> ~238Hz refresh.
const SLOT_US: u32 = 100;

const WIDTH: usize = 6;
const HEIGHT: usize = 7;
const PIXELS: usize = WIDTH * HEIGHT;

const RED: usize = 0;
const GREEN: usize = 1;
const BLUE: usize = 2;
const COLORS: usize = 3;

/// Charlieplex pins, index = net number (xLED0..xLED6).
const RED_PINS: [u8; 7] = [0, 1, 2, 3, 4, 5, 6]; // PA
const GREEN_PINS: [u8; 7] = [7, 8, 9, 10, 11, 12, 15]; // PA
const BLUE_PINS: [u8; 7] = [2, 3, 4, 5, 6, 7, 8]; // PB

/// Each net is the anode of one physical row (6 LEDs), whose cathodes walk
/// through the other nets left to right. Anode net of row `y`, row 0 is the
/// top one.
const ROW_NET: [usize; HEIGHT] = [6, 3, 4, 5, 2, 1, 0];

/// One colour's matrix in screen order: LED `y * WIDTH + x`.
const fn matrix(port: Port, pins: &[u8; 7]) -> [Led; PIXELS] {
    let mut leds = [Led::new(port, pins[0], pins[1]); PIXELS];
    let mut i = 0;
    while i < PIXELS {
        let (x, y) = (i % WIDTH, i / WIDTH);
        leds[i] = Led::canonical(port, pins, ROW_NET[y] * WIDTH + x);
        i += 1;
    }
    leds
}

static RED_LEDS: [Led; PIXELS] = matrix(Port::A, &RED_PINS);
static GREEN_LEDS: [Led; PIXELS] = matrix(Port::A, &GREEN_PINS);
static BLUE_LEDS: [Led; PIXELS] = matrix(Port::B, &BLUE_PINS);

/// LED index = `color * PIXELS + y * WIDTH + x`.
static LED_GROUPS: [&[Led]; COLORS] = [&RED_LEDS, &GREEN_LEDS, &BLUE_LEDS];

/// BUT (SW100) on PF4/BOOT0, to ground.
const WAKE: WakePin = WakePin::new(Port::F, 4);

/// Sets a whole column of one colour, bit `y` of `bits` = row `y` (bit 0 is
/// the top row).
fn set_column(color: usize, x: usize, bits: u8) {
    for y in 0..HEIGHT {
        charlieplex::set(color * PIXELS + y * WIDTH + x, bits & (1 << y) != 0);
    }
}

/// Sets a whole column, `rgb` is a bitmask of the colours to light
/// (bit `RED`, `GREEN`, `BLUE`).
fn set_column_rgb(rgb: u8, x: usize, bits: u8) {
    for color in 0..COLORS {
        set_column(color, x, if rgb & (1 << color) != 0 { bits } else { 0 });
    }
}

// ---- Behaviour -------------------------------------------------------------

const MESSAGE: &[u8] = b"HELLO WORLD";

/// The badge turns itself off after this long without a button press.
const AWAKE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

const WIPE_STEP: Duration = Duration::from_millis(40);
const SCROLL_STEP: Duration = Duration::from_millis(70);

/// Reason an animation ended early: button pressed or awake time used up.
struct Off;

struct Show<'a, 'd> {
    button: &'a mut Button<'d>,
    deadline: Instant,
}

impl Show<'_, '_> {
    /// Delay between animation steps.
    async fn step(&mut self, duration: Duration) -> Result<(), Off> {
        if self.button.pressed_within(duration).await || Instant::now() >= self.deadline {
            Err(Off)
        } else {
            Ok(())
        }
    }

    /// Fills the matrix column by column in each colour, then empties it the
    /// same way. Lights every LED die once, so it doubles as an assembly
    /// check.
    async fn wipe(&mut self) -> Result<(), Off> {
        for color in [RED, GREEN, BLUE] {
            for bits in [0x7F, 0x00] {
                for x in 0..WIDTH {
                    set_column(color, x, bits);
                    self.step(WIPE_STEP).await?;
                }
            }
        }
        Ok(())
    }

    /// Scrolls the message in from the right edge and out over the left one.
    async fn scroll_message(&mut self, rgb: u8) -> Result<(), Off> {
        let message_cols = font::text_width(MESSAGE);

        for offset in 0..message_cols + WIDTH {
            for x in 0..WIDTH {
                // The message starts one screen width to the right.
                let bits = match (offset + x).checked_sub(WIDTH) {
                    Some(i) if i < message_cols => font::text_column(MESSAGE, i),
                    _ => 0,
                };
                set_column_rgb(rgb, x, bits);
            }
            self.step(SCROLL_STEP).await?;
        }
        Ok(())
    }

    async fn run(&mut self) -> Result<(), Off> {
        loop {
            // Every colour mix but black: R, G, R+G, B, ...
            for rgb in 1..8 {
                self.wipe().await?;
                self.scroll_message(rgb).await?;
                charlieplex::clear();
            }
        }
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Recovery window: stay awake with every pin in its reset state for a
    // second or two (HSI reset default, before any clock setup), so a probe
    // can always attach right after power-up even if the code below ever
    // breaks SWD or sleeps too deep. Keep this first.
    cortex_m::asm::delay(8_000_000);

    let mut cfg: py32_hal::Config = Default::default();
    cfg.rcc.hsi = Some(Hertz::mhz(SYSCLK_HZ / 1_000_000));
    cfg.rcc.sys = Sysclk::HSI;
    let p = py32_hal::init(cfg);

    info!("Hello World!");

    // PA13/PA14 (SWD) and PF2 (nRST) are left alone on purpose.
    let syst = cortex_m::Peripherals::take().unwrap().SYST;
    charlieplex::init(syst, SYSCLK_HZ, SLOT_US, &LED_GROUPS);

    // Pulled up on the board as well.
    let mut button = Button::new(Input::new(p.PF4, Pull::Up));

    loop {
        let mut show = Show {
            button: &mut button,
            deadline: Instant::now() + AWAKE_TIMEOUT,
        };
        let _ = show.run().await;

        info!("off");
        charlieplex::stop();
        charlieplex::clear();
        button.wait_release().await;
        power::stop(WAKE);
        button.wait_release().await;
        info!("on");
        charlieplex::start();
    }
}

//conserve flash space with the non-handler
#[exception]
unsafe fn HardFault(_frame: &ExceptionFrame) -> ! {
    SCB::sys_reset()
}
