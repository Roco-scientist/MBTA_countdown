use chrono;
use chrono::prelude::*;

pub struct Clocks {
    display_ht16k33: Option<crate::ht16k33::ClockDisplay>,
    display_tm1637: Option<crate::tm1637::ClockDisplay>,
}

impl Clocks {
    pub fn new(
        clock_type: String,
        clock_brightness: u8,
        address: Option<u8>,
    ) -> Result<Clocks, Box<dyn std::error::Error>> {
        if clock_type == "HT16K33".to_string() {
            let clock_ht16k33 =
                crate::ht16k33::ClockDisplay::new(address.unwrap(), clock_brightness)?;
            Ok(Clocks {
                display_ht16k33: Some(clock_ht16k33),
                display_tm1637: None,
            })
        } else {
            let clock_tm1637 = crate::tm1637::ClockDisplay::new(clock_brightness)?;
            Ok(Clocks {
                display_ht16k33: None,
                display_tm1637: Some(clock_tm1637),
            })
        }
    }
    pub fn display_time_until(
        &mut self,
        train_times_list: &Vec<chrono::DateTime<Local>>,
        minimum_display_min: &i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(display) = &mut self.display_ht16k33 {
            display.display_time_until(&train_times_list, &minimum_display_min)?
        } else {
            if let Some(display) = &mut self.display_tm1637 {
                display.display_time_until(&train_times_list, &minimum_display_min)?
            }
        }
        Ok(())
    }

    pub fn clear_display(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(display) = &mut self.display_ht16k33 {
            display.clear_display()?
        } else {
            if let Some(display) = &mut self.display_tm1637 {
                display.clear_display()?
            }
        }
        Ok(())
    }
}
