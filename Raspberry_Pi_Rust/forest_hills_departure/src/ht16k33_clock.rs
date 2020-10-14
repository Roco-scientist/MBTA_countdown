extern crate chrono;
extern crate ht16k33;
extern crate rppal;
extern crate std;

use chrono::prelude::*;
use rppal::i2c::I2c;
use std::collections::HashMap;

lazy_static! {
// Below are the led numbers for each number within the clock
//   _   0
//  |_|  5, 6, 1
//  |_|  4, 3, 2
//
//  Create a hashmap below with each number and the corresponding leds that need to be
//  turned on
    static ref NUMBER_LEDS: HashMap<u8, Vec<u8>> = [
        (0u8, vec![0u8, 1u8, 2u8, 3u8, 4u8, 5u8]),
        (1u8, vec![1u8, 2u8]),
        (2u8, vec![0u8, 1u8, 3u8, 4u8, 6u8]),
        (3u8, vec![0u8, 1u8, 2u8, 3u8, 6u8]),
        (4u8, vec![1u8, 2u8, 5u8, 6u8]),
        (5u8, vec![0u8, 2u8, 3u8, 5u8, 6u8]),
        (6u8, vec![0u8, 2u8, 3u8, 4u8, 5u8, 6u8]),
        (7u8, vec![0u8, 1u8, 2u8]),
        (8u8, vec![0u8, 1u8, 2u8, 3u8, 4u8, 5u8, 6u8]),
        (9u8, vec![0u8, 1u8, 2u8, 3u8, 5u8, 6u8]),
    ]
    .iter()
    .cloned()
    .collect();
}

/// A struct to hold the display along with the digits for each location
pub struct ClockDisplay {
    display: ht16k33::HT16K33<I2c>,
    minutes_ten: Option<u8>,
    minutes_single: Option<u8>,
    seconds_ten: Option<u8>,
    seconds_single: Option<u8>,
}

// Functions to initialize and change clock display
impl ClockDisplay {
    /// Creates a new ClockDisplay struct
    pub fn new() -> ClockDisplay {
        // create new i2c interface
        let i2c = I2c::new().unwrap();
        // connect to the I2C address found within raspberry pi.  This is converted to decimal
        // actually 0x70 in hexidecimal which goes to 112
        let address = 112u8;
        // connect the ht16k33 clock chip to i2c connection on the address
        let mut clock = ht16k33::HT16K33::new(i2c, address);
        clock.initialize().unwrap();
        // turn clock display on.  Would not work otherwise
        clock.set_display(ht16k33::Display::ON).unwrap();
        // set the dimming of the display.  This can be added to new function later
        clock
            .set_dimming(ht16k33::Dimming::from_u8(7u8).unwrap())
            .unwrap();
        // return ClockDisplay struct with empty digits to be filled later
        ClockDisplay {
            display: clock,
            minutes_ten: None,
            minutes_single: None,
            seconds_ten: None,
            seconds_single: None,
        }
    }

    /// Dispalys the minutes:seconds until the next train on the clock display
    pub fn display_time_until(&mut self, train_time: &chrono::DateTime<Local>) -> () {
            // get now time in UTC
            let now = chrono::Local::now();
            // get the difference between now and the train time
            let diff = train_time.signed_duration_since(now);
            // separate out minutes and seconds for the display
            // if minutes are above 250, reduce so it can be a u8
            let minutes;
            if diff.num_minutes() < 250i64 {
                minutes = diff.num_minutes() as u8;
            } else {
                minutes = 250u8;
            }
            // Seconds as the remainder after minutes are removed
            let seconds = (diff.num_seconds() % 60i64) as u8;
            // Clock display only has two digits for minutes, so minutes need to be below 100
            if minutes < 100u8 {
                // find all of the new digits for displaying difference
                // first digit, which is the tens minutes
                let first = minutes / 10u8;
                // second digit, which is the single minutes
                let second = minutes % 10u8;
                // third digit, which is the seconds ten
                let third = seconds / 10u8;
                // fourth digit, which is the seconds single
                let fourth = seconds % 10u8;
                // if current display has no values, then display all of the new values
                if self.minutes_ten.is_none() {
                    self.minutes_ten = Some(first);
                    self.minutes_single = Some(second);
                    self.seconds_ten = Some(third);
                    self.seconds_single = Some(fourth);
                    self.display_nums();
                } else {
                    // else change only the values that have changed
                    if Some(first) != self.minutes_ten {
                        self.change_number(0, &first);
                        self.minutes_ten = Some(first);
                    }
                    if Some(second) != self.minutes_single {
                        self.change_number(2, &second);
                        self.minutes_single = Some(second);
                    }
                    if Some(third) != self.seconds_ten {
                        self.change_number(6, &third);
                        self.seconds_ten = Some(third);
                    }
                    if Some(fourth) != self.seconds_single {
                        self.change_number(8, &fourth);
                        self.seconds_single = Some(fourth);
                    }
                }
            } else {
                // if minutes is greater than 100 clear dispaly and set all values to none
                self.minutes_ten = None;
                self.minutes_single = None;
                self.seconds_ten = None;
                self.seconds_single = None;
                self.clear_display();
            }
    }

    /// Clears clock display
    pub fn clear_display(&mut self) -> () {
        // clear the display buffer then push to clock to create a clear clock
        self.display.clear_display_buffer();
        self.display.write_display_buffer().unwrap();
    }

    /// Turns on all numbers
    fn display_nums(&mut self) -> () {
        // Retrieve a vec! of leds that need to be turned on for the numbers
        // Then turn them on
        let leds = NUMBER_LEDS.get(&self.minutes_ten.unwrap()).unwrap();
        self.turn_on_leds(leds, 0);
        let leds = NUMBER_LEDS.get(&self.minutes_single.unwrap()).unwrap();
        self.turn_on_leds(leds, 2);
        let leds = NUMBER_LEDS.get(&self.seconds_ten.unwrap()).unwrap();
        self.turn_on_leds(leds, 6);
        let leds = NUMBER_LEDS.get(&self.seconds_single.unwrap()).unwrap();
        self.turn_on_leds(leds, 8);
        self.display_colon(true);
    }

    /// Turns on the necessary leds for a number at the indicated location
    fn turn_on_leds(&mut self, leds: &Vec<u8>, location: u8) -> () {
        // Turn on/off each led
        for led in leds {
            let led_location = ht16k33::LedLocation::new(location, *led).unwrap();
            self.display.set_led(led_location, true).unwrap();
        }
    }

    /// Turns on/off the colon between the digits for the clock
    fn display_colon(&mut self, on: bool) -> () {
        let leds = vec![0u8, 1u8];
        // Turn on/off each led
        for led in leds {
            // colon is located at location 4, with leds 1,2
            let led_location = ht16k33::LedLocation::new(4u8, led).unwrap();
            self.display.set_led(led_location, on).unwrap();
        }
    }

    fn change_number(&mut self, location: u8, new_number: &u8) {
        // determine which struct digit to pull based on led location
        let old_number = match location {
            0u8 => self.minutes_ten.unwrap().clone(),
            2u8 => self.minutes_single.unwrap().clone(),
            6u8 => self.seconds_ten.unwrap().clone(),
            8u8 => self.seconds_single.unwrap().clone(),
            _ => panic!("location not recognized"),
        };
        // get the leds for th old number
        let old_leds = NUMBER_LEDS.get(&old_number).unwrap();
        // get the leds for the new number
        let new_leds = NUMBER_LEDS.get(new_number).unwrap();
        // get what leds are in the old number and not the new to then be able to turn off
        let leds_off = old_leds.iter().filter(|led| !new_leds.contains(led));
        // get what leds are in the new number but no tht eold to then be able to turn on
        // these two are used so that instead of turning all off then new on, only switching the
        // necessary leds
        let leds_on = new_leds.iter().filter(|led| !old_leds.contains(led));
        // turn off leds
        for led in leds_off {
            let led_location = ht16k33::LedLocation::new(location, *led).unwrap();
            self.display.set_led(led_location, false).unwrap();
        }
        // turn on leds
        for led in leds_on {
            let led_location = ht16k33::LedLocation::new(location, *led).unwrap();
            self.display.set_led(led_location, true).unwrap();
        }
    }
}