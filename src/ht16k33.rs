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
    pub fn new(
        address: u8,
        clock_brightness: u8,
    ) -> Result<ClockDisplay, Box<dyn std::error::Error>> {
        // create new i2c interface
        let i2c = I2c::new()?;
        // connect the ht16k33 clock chip to i2c connection on the address
        let mut clock = ht16k33::HT16K33::new(i2c, address);
        clock.initialize()?;
        // turn clock display on.  Would not work otherwise
        clock.set_display(ht16k33::Display::ON)?;
        // set the dimming of the display.  This can be added to new function later
        clock.set_dimming(ht16k33::Dimming::from_u8(clock_brightness)?)?;
        // return ClockDisplay struct with empty digits to be filled later
        Ok(ClockDisplay {
            display: clock,
            minutes_ten: None,
            minutes_single: None,
            seconds_ten: None,
            seconds_single: None,
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
                    self.clear_display()?;
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
            let first = (minutes as u8) / 10u8;
            // second digit, which is the single minutes
            let second = (minutes as u8) % 10u8;
            // third digit, which is the seconds ten
            let third = (seconds as u8) / 10u8;
            // fourth digit, which is the seconds single
            let fourth = (seconds as u8) % 10u8;
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
                if Some(first) != self.minutes_ten {
                    self.change_number(0, &first)?;
                    self.minutes_ten = Some(first);
                }
                if Some(second) != self.minutes_single {
                    self.change_number(2, &second)?;
                    self.minutes_single = Some(second);
                }
                if Some(third) != self.seconds_ten {
                    self.change_number(6, &third)?;
                    self.seconds_ten = Some(third);
                }
                if Some(fourth) != self.seconds_single {
                    self.change_number(8, &fourth)?;
                    self.seconds_single = Some(fourth);
                }
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
        self.display.clear_display_buffer();
        self.display.write_display_buffer()?;
        Ok(())
    }

    /// Turns on all numbers
    fn display_nums(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Retrieve a vec! of leds that need to be turned on for the numbers
        // Then turn them on
        if let Some(minutes_ten) = self.minutes_ten {
            let leds = NUMBER_LEDS.get(&minutes_ten).unwrap();
            self.switch_leds(leds, 0, true)?;
        }
        if let Some(minutes_single) = self.minutes_single {
            let leds = NUMBER_LEDS.get(&minutes_single).unwrap();
            self.switch_leds(leds, 2, true)?;
        }
        if let Some(seconds_ten) = self.seconds_ten {
            let leds = NUMBER_LEDS.get(&seconds_ten).unwrap();
            self.switch_leds(leds, 6, true)?;
        }
        if let Some(seconds_single) = self.seconds_single {
            let leds = NUMBER_LEDS.get(&seconds_single).unwrap();
            self.switch_leds(leds, 8, true)?;
        }
        self.display_colon(true)?;
        Ok(())
    }

    /// Turns on/off the necessary leds for a number at the indicated location
    fn switch_leds(
        &mut self,
        leds: &[u8],
        location: u8,
        on: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Turn on/off each led
        for led in leds {
            let led_location = ht16k33::LedLocation::new(location, *led)?;
            self.display.set_led(led_location, on)?;
        }
        Ok(())
    }

    /// Turns on/off the colon between the digits for the clock
    fn display_colon(&mut self, on: bool) -> Result<(), Box<dyn std::error::Error>> {
        let leds = vec![0u8, 1u8];
        // Turn on/off each led
        for led in leds {
            // colon is located at location 4, with leds 1,2
            let led_location = ht16k33::LedLocation::new(4u8, led)?;
            self.display.set_led(led_location, on)?;
        }
        Ok(())
    }

    fn change_number(
        &mut self,
        location: u8,
        new_number: &u8,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // determine which struct digit to pull based on led location
        let old_number_option = match location {
            0u8 => self.minutes_ten,
            2u8 => self.minutes_single,
            6u8 => self.seconds_ten,
            8u8 => self.seconds_single,
            _ => panic!("location not recognized"),
        };
        // get the leds for the new number
        let new_leds = NUMBER_LEDS.get(new_number).unwrap();
        if let Some(old_number) = old_number_option {
            // get the leds for th old number
            let old_leds = NUMBER_LEDS.get(&old_number).unwrap();
            // get what leds are in the old number and not the new to then be able to turn off
            let leds_off = old_leds
                .iter()
                .filter_map(|led| {
                    if !new_leds.contains(led) {
                        Some(*led)
                    } else {
                        None
                    }
                })
                .collect::<Vec<u8>>();
            // get what leds are in the new number but no tht eold to then be able to turn on
            // these two are used so that instead of turning all off then new on, only switching the
            // necessary leds
            let leds_on = new_leds
                .iter()
                .filter_map(|led| {
                    if !old_leds.contains(led) {
                        Some(*led)
                    } else {
                        None
                    }
                })
                .collect::<Vec<u8>>();
            // turn off leds
            self.switch_leds(&leds_off, location, false)?;
            // turn on leds
            self.switch_leds(&leds_on, location, true)?
        } else {
            self.switch_leds(new_leds, location, true)?
        };
        Ok(())
    }
}
