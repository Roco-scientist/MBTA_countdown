use chrono;
use chrono::prelude::*;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use rppal;
use std;
use std::thread::sleep;
use std::time::Duration;

const FREQUENCY_KHZ: u64 = 10; // 250 max
const DELAY_USECS: u64 = 1000 / FREQUENCY_KHZ; //USECS_IN_MSEC / MAX_FREQ_KHZ;

const _ADDRESS_AUTO_INCREMENT_1_MODE: u8 = 0b0100_0000; // 0x40;
const FIXED_ADDRESS_MODE: u8 = 0b0100_0100; // 0x40;

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
    0b0011_1111,
    0b0000_0110,
    0b0101_1011,
    0b0100_1111,
    0b0110_0110,
    0b0110_1101,
    0b0111_1101,
    0b0000_0111,
    0b0111_1111,
    0b0110_1111,
    0b0111_0111,
    0b0111_1100,
    0b0011_1001,
    0b0101_1110,
    0b0111_1001,
    0b0111_0001,
];

const DISPLAY_ADDRESS: [u8; 4] = [0b1100_0000, 0b1100_0001, 0b1100_0010, 0b1100_0011];

const _TURN_ON: u8 = 0b1000_1000;
const TURN_OFF: u8 = 0b1000_0000;

const DISPLAY_BRIGHTNESS: [u8; 8] = [
    0b1000_1000,
    0b1000_1001,
    0b1000_1010,
    0b1000_1011,
    0b1000_1100,
    0b1000_1101,
    0b1000_1110,
    0b1000_1111,
]; // page 5 of spec sheet

/// A struct to hold the display along with the digits for each location
pub struct ClockDisplay {
    display: TM1637<rppal::gpio::OutputPin, rppal::gpio::OutputPin>,
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
        let gpio = rppal::gpio::Gpio::new()?;
        let clk_pin = gpio.get(22)?.into_output();
        let dio_pin = gpio.get(27)?.into_output();
        // connect the ht16k33 clock chip to i2c connection on the address
        let clock = TM1637::new(clk_pin, dio_pin);
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
        train_times: &[chrono::DateTime<Local>],
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
                self.display.command_one(FIXED_ADDRESS_MODE).unwrap();
                if Some(first) != self.minutes_ten {
                    self.display
                        .print_raw(DISPLAY_ADDRESS[0], BINS[first])
                        .unwrap();
                    self.minutes_ten = Some(first);
                }
                if Some(second) != self.minutes_single {
                    let mut bin_colon = BINS[second as usize];
                    // add the colon with the first bit
                    bin_colon |= 0b10000000;
                    self.display
                        .print_raw(DISPLAY_ADDRESS[1], bin_colon)
                        .unwrap();
                    self.minutes_single = Some(second);
                }
                if Some(third) != self.seconds_ten {
                    self.display
                        .print_raw(DISPLAY_ADDRESS[2], BINS[third])
                        .unwrap();
                    self.seconds_ten = Some(third);
                }
                if Some(fourth) != self.seconds_single {
                    self.display
                        .print_raw(DISPLAY_ADDRESS[3], BINS[fourth])
                        .unwrap();
                    self.seconds_single = Some(fourth);
                }
                self.display
                    .command_three_control_display(self.brightness)
                    .unwrap();
            }
        } else {
            // if minutes is greater than 100 clear dispaly and set all values to none
            self.clear_display()?;
        };
        Ok(())
    }

    /// Clears clock display
    pub fn clear_display(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        //set all values to None
        self.minutes_ten = None;
        self.minutes_single = None;
        self.seconds_ten = None;
        self.seconds_single = None;
        // clear the display buffer then push to clock to create a clear clock
        self.display.command_one(FIXED_ADDRESS_MODE).unwrap();
        self.display
            .print_raw(DISPLAY_ADDRESS[0], 0b0000_0000)
            .unwrap();
        self.display
            .print_raw(DISPLAY_ADDRESS[1], 0b0000_0000)
            .unwrap();
        self.display
            .print_raw(DISPLAY_ADDRESS[2], 0b0000_0000)
            .unwrap();
        self.display
            .print_raw(DISPLAY_ADDRESS[3], 0b0000_0000)
            .unwrap();
        self.display.command_three_turn_off().unwrap();
        // self.display
        //     .command_three_control_display(self.brightness)
        //     .unwrap();
        Ok(())
    }

    /// Turns on all numbers
    fn display_nums(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Retrieve a vec! of leds that need to be turned on for the numbers
        // Then turn them on
        self.display.command_one(FIXED_ADDRESS_MODE).unwrap();
        if let Some(minutes_ten) = self.minutes_ten {
            self.display
                .print_raw(DISPLAY_ADDRESS[0], BINS[minutes_ten])
                .unwrap();
        }
        if let Some(minutes_single) = self.minutes_single {
            let mut bin_colon = BINS[minutes_single as usize];
            // add the colon with the first bit
            bin_colon |= 0b10000000;
            self.display
                .print_raw(DISPLAY_ADDRESS[1], bin_colon)
                .unwrap();
        }
        if let Some(seconds_ten) = self.seconds_ten {
            self.display
                .print_raw(DISPLAY_ADDRESS[2], BINS[seconds_ten])
                .unwrap();
        }
        if let Some(seconds_single) = self.seconds_single {
            self.display
                .print_raw(DISPLAY_ADDRESS[3], BINS[seconds_single])
                .unwrap();
        }
        self.display
            .command_three_control_display(self.brightness)
            .unwrap();
        Ok(())
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

pub struct TM1637<CLK, DIO> {
    clk: CLK,
    dio: DIO,
}

enum Bit {
    Zero,
    One,
}

impl<CLK, DIO, E> TM1637<CLK, DIO>
where
    CLK: OutputPin<Error = E>,
    DIO: InputPin<Error = E> + OutputPin<Error = E>,
{
    pub fn new(clk: CLK, dio: DIO) -> Self {
        Self { clk, dio }
    }

    pub fn command_one(&mut self, mode: u8) -> Res<E> {
        self.start()?;
        self.send(mode)?;
        self.stop()?;

        Ok(())
    }

    pub fn print_raw(&mut self, address: u8, byte: u8) -> Res<E> {
        self.start()?;
        // send command 2
        self.send(address)?;
        // send data command
        self.send(byte)?;
        self.stop()?;
        Ok(())
    }

    pub fn command_three_control_display(&mut self, level: usize) -> Res<E> {
        self.start()?;
        self.send(DISPLAY_BRIGHTNESS[level])?;
        self.stop()?;

        Ok(())
    }

    pub fn command_three_turn_off(&mut self) -> Res<E> {
        self.start()?;
        self.send(TURN_OFF)?;
        self.stop()?;

        Ok(())
    }

    fn send(&mut self, byte: u8) -> Res<E> {
        let mut rest = byte;
        for _ in 0..8 {
            let bit = if rest & 1 != 0 { Bit::One } else { Bit::Zero };
            self.send_bit_and_delay(bit)?;
            rest >>= 1;
        }

        // Wait for the ACK
        self.clk.set_low()?;
        self.short_delay();
        // self.dio.set_low()?;
        // self.delay();
        self.dio.set_high()?;
        self.long_delay();
        self.clk.set_high()?;
        self.delay();
        for _ in 0..10 {
            if self.dio.is_low()? {
                return Ok(());
            }
            let short_delay = DELAY_USECS / 10;
            sleep(Duration::from_micros(short_delay));
        }

        // println!("Ack failed");
        Ok(())
        // Err(Error::Ack)
    }

    fn start(&mut self) -> Res<E> {
        self.dio.set_high()?;
        self.clk.set_high()?;
        self.delay();
        self.dio.set_low()?;
        self.delay();
        Ok(())
    }

    fn stop(&mut self) -> Res<E> {
        self.clk.set_low()?;
        self.short_delay();
        self.dio.set_low()?;
        self.long_delay();
        self.clk.set_high()?;
        self.delay();
        self.dio.set_high()?;
        self.delay();

        Ok(())
    }

    fn send_bit_and_delay(&mut self, value: Bit) -> Res<E> {
        self.clk.set_low()?;
        self.short_delay();
        if let Bit::One = value {
            self.dio.set_high()?;
        } else {
            self.dio.set_low()?;
        }
        self.long_delay();
        self.clk.set_high()?;
        self.delay();

        Ok(())
    }

    fn delay(&mut self) {
        sleep(Duration::from_micros(DELAY_USECS));
    }

    fn short_delay(&mut self) {
        let delay_time = DELAY_USECS / 10;
        sleep(Duration::from_micros(delay_time));
    }

    fn long_delay(&mut self) {
        let delay_time = DELAY_USECS * 9 / 10;
        sleep(Duration::from_micros(delay_time));
    }
}
