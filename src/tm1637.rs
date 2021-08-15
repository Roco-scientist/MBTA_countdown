use chrono;
use chrono::prelude::*;
use embedded_hal::blocking::delay::DelayUs;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use rppal;
use std;

const FREQUENCY_KHZ: u16 = 50; // 250 max
const DELAY_USECS: u16 = 1000/FREQUENCY_KHZ; //USECS_IN_MSEC / MAX_FREQ_KHZ;

const _ADDRESS_AUTO_INCREMENT_1_MODE: u8 = 0b01000000; // 0x40;
const FIXED_ADDRESS_MODE: u8 = 0b01000100; // 0x40;

//      A
//     ---
//  F |   | B   *
//     -G-      H (on 2nd segment)
//  E |   | C   *
//     ---
//      D
//
//   HGFEDCBA
// 0b01101101 = 0x6D = 109 = show "5"
const BINS: [u8; 16] = [
    0b00111111, 0b00000110, 0b01011011, 0b01001111, 0b01100110, 0b01101101, 0b01111101, 0b00000111,
    0b01111111, 0b01101111, 0b01110111, 0b01111100, 0b00111001, 0b01011110, 0b01111001, 0b01110001,
];

const DISPLAY_ADDRESS: [u8; 4] = [
    0b11000000, 0b11000001, 0b11000010, 0b11000011, 
]; 

const DISPLAY_BRIGHTNESS: [u8; 8] = [
    0b10001000, 0b10001001, 0b10001010, 0b10001011, 0b10001100, 0b10001101, 0b10001110, 0b10001111, 
];  // page 5 of spec sheet

/// A struct to hold the display along with the digits for each location
pub struct ClockDisplay {
    display: TM1637<rppal::gpio::OutputPin, rppal::gpio::OutputPin, rppal::hal::Delay>,
    minutes_ten: Option<usize>,
    minutes_single: Option<usize>,
    seconds_ten: Option<usize>,
    seconds_single: Option<usize>,
    brightness: usize,
}

// Functions to initialize and change clock display
impl ClockDisplay {
    /// Creates a new ClockDisplay struct
    pub fn new(clock_brightness: u8) -> Result<ClockDisplay, Box<dyn std::error::Error>> {
        // create new i2c interface
        let delay = rppal::hal::Delay::new();
        // let mut us_delay = delay.delay_us(100u8);
        let gpio = rppal::gpio::Gpio::new()?;
        let clk_pin = gpio.get(22)?.into_output();
        let dio_pin = gpio.get(27)?.into_output();
        // connect the ht16k33 clock chip to i2c connection on the address
        let mut clock = TM1637::new(clk_pin, dio_pin, delay);
        clock.init().unwrap();
        clock.clear().unwrap();
        // return ClockDisplay struct with empty digits to be filled later
        Ok(ClockDisplay {
            display: clock,
            minutes_ten: None,
            minutes_single: None,
            seconds_ten: None,
            seconds_single: None,
            brightness: clock_brightness as usize,
        })
    }

    /// Dispalys the minutes:seconds until the next train on the clock display
    pub fn display_time_until(
        &mut self,
        train_times: &Vec<chrono::DateTime<Local>>,
        minimum_display_min: &i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // get now time in UTC
        let now = chrono::Local::now();
        // get the difference between now and the train time
        let mut diff = train_times[0].signed_duration_since(now);
        // if difference is less than minumum display, use next train
        if diff.num_minutes() < *minimum_display_min {
            if train_times.len() > 1usize {
                diff = train_times[1].signed_duration_since(now)
            } else {
                // if there is not a next train, clear display and end
                if vec![
                    self.minutes_ten,
                    self.minutes_single,
                    self.seconds_ten,
                    self.seconds_single,
                ]
                .iter()
                .any(|digit| digit.is_some())
                {
                    self.clear_display().unwrap();
                };
                return Ok(());
            }
        }
        // separate out minutes and seconds for the display
        let minutes = diff.num_minutes();
        // Seconds as the remainder after minutes are removed
        let seconds = diff.num_seconds() % 60i64;
        // Clock display only has two digits for minutes, so minutes need to be below 100
        if *minimum_display_min < minutes && minutes < 100i64 {
            // find all of the new digits for displaying difference
            // first digit, which is the tens minutes
            let first = (minutes as usize) / 10usize;
            // second digit, which is the single minutes
            let second = (minutes as usize) % 10usize;
            // third digit, which is the seconds ten
            let third = (seconds as usize) / 10usize;
            // fourth digit, which is the seconds single
            let fourth = (seconds as usize) % 10usize;
            // if current display has no values, then display all of the new values
            if vec![
                self.minutes_ten,
                self.minutes_single,
                self.seconds_ten,
                self.seconds_single,
            ]
            .iter()
            .any(|digit| digit.is_none())
            {
                self.minutes_ten = Some(first);
                self.minutes_single = Some(second);
                self.seconds_ten = Some(third);
                self.seconds_single = Some(fourth);
                self.display_nums()?;
            } else {
                // else change only the values that have changed
                self.display.init().unwrap();
                if Some(first) != self.minutes_ten {
                    self.display.print_raw(DISPLAY_ADDRESS[0], BINS[first]).unwrap();
                    self.minutes_ten = Some(first);
                }
                if Some(second) != self.minutes_single {
                    let mut bin_colon = BINS[second as usize];
                    // add the colon with the first bit
                    bin_colon |= 0b10000000;
                    self.display.print_raw(DISPLAY_ADDRESS[1], bin_colon).unwrap();
                    self.minutes_single = Some(second);
                }
                if Some(third) != self.seconds_ten {
                    self.display.print_raw(DISPLAY_ADDRESS[2], BINS[third]).unwrap();
                    self.seconds_ten = Some(third);
                }
                if Some(fourth) != self.seconds_single {
                    self.display.print_raw(DISPLAY_ADDRESS[3], BINS[fourth]).unwrap();
                    self.seconds_single = Some(fourth);
                }
                self.display.set_brightness(self.brightness).unwrap();
            }
        } else {
            // if minutes is greater than 100 clear dispaly and set all values to none
            self.clear_display()?;
        };
        return Ok(());
    }

    /// Clears clock display
    pub fn clear_display(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        //set all values to None
        self.minutes_ten = None;
        self.minutes_single = None;
        self.seconds_ten = None;
        self.seconds_single = None;
        // clear the display buffer then push to clock to create a clear clock
        self.display.clear().unwrap();
        Ok(())
    }

    /// Turns on all numbers
    fn display_nums(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Retrieve a vec! of leds that need to be turned on for the numbers
        // Then turn them on
        self.display.init().unwrap();
        if let Some(minutes_ten) = self.minutes_ten {
            self.display.print_raw(DISPLAY_ADDRESS[0], BINS[minutes_ten]).unwrap();
        }
        if let Some(minutes_single) = self.minutes_single {
            let mut bin_colon = BINS[minutes_single as usize];
            // add the colon with the first bit
            bin_colon |= 0b10000000;
            self.display.print_raw(DISPLAY_ADDRESS[1], bin_colon).unwrap();
        }
        if let Some(seconds_ten) = self.seconds_ten {
            self.display.print_raw(DISPLAY_ADDRESS[2], BINS[seconds_ten]).unwrap();
        }
        if let Some(seconds_single) = self.seconds_single {
            self.display
                .print_raw(DISPLAY_ADDRESS[3], BINS[seconds_single])
                .unwrap();
        }
        self.display.set_brightness(self.brightness).unwrap();
        return Ok(());
    }
}

#[derive(Debug)]
pub enum Error<E> {
    Ack,
    IO(E),
}

impl<E> From<E> for Error<E> {
    fn from(err: E) -> Error<E> {
        Error::IO(err)
    }
}

type Res<E> = Result<(), Error<E>>;

pub struct TM1637<CLK, DIO, D> {
    clk: CLK,
    dio: DIO,
    delay: D,
}

enum Bit {
    ZERO,
    ONE,
}

impl<CLK, DIO, D, E> TM1637<CLK, DIO, D>
where
    CLK: OutputPin<Error = E>,
    DIO: InputPin<Error = E> + OutputPin<Error = E>,
    D: DelayUs<u16>,
{
    pub fn new(clk: CLK, dio: DIO, delay: D) -> Self {
        Self { clk, dio, delay }
    }

    pub fn init(&mut self) -> Res<E> {
        self.start()?;
        self.send(FIXED_ADDRESS_MODE)?;
        self.stop()?;

        Ok(())
    }

    pub fn clear(&mut self) -> Res<E> {
        self.print_raw_iter(0, core::iter::repeat(0).take(4))
    }

    pub fn print_raw(&mut self, address: u8, byte: u8) -> Res<E> {
        self.start()?;
        self.send(address)?;
        self.send(byte)?;
        self.stop()?;
        Ok(())
    }

    pub fn print_raw_iter<Iter: Iterator<Item = u8>>(
        &mut self,
        address: u8,
        bytes: Iter,
    ) -> Res<E> {
        self.start()?;
        self.send(address)?;
        for byte in bytes {
            self.send(byte)?;
        }
        self.stop()?;
        Ok(())
    }

    pub fn set_brightness(&mut self, level: usize) -> Res<E> {
        self.start()?;
        self.send(DISPLAY_BRIGHTNESS[level])?;
        self.stop()?;

        Ok(())
    }

    fn send(&mut self, byte: u8) -> Res<E> {
        let mut rest = byte;
        for _ in 0..8 {
            let bit = if rest & 1 != 0 { Bit::ONE } else { Bit::ZERO };
            self.send_bit_and_delay(bit)?;
            rest = rest >> 1;
        }

        // Wait for the ACK
        self.send_bit_and_delay(Bit::ONE)?;
        for _ in 0..255 {
            if self.dio.is_low()? {
                return Ok(());
            }
            self.delay();
        }

        // println!("Ack failed");
        Ok(())
        // Err(Error::Ack)
    }

    fn start(&mut self) -> Res<E> {
        self.send_bit_and_delay(Bit::ONE)?;
        self.dio.set_low()?;

        Ok(())
    }

    fn stop(&mut self) -> Res<E> {
        self.send_bit_and_delay(Bit::ZERO)?;
        self.dio.set_high()?;
        self.delay();

        Ok(())
    }

    fn send_bit_and_delay(&mut self, value: Bit) -> Res<E> {
        self.clk.set_low()?;
        if let Bit::ONE = value {
            self.dio.set_high()?;
        } else {
            self.dio.set_low()?;
        }
        self.clk.set_high()?;
        self.delay();

        Ok(())
    }

    fn delay(&mut self) {
        self.delay.delay_us(DELAY_USECS);
    }
}
