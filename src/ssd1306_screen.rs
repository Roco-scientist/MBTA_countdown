extern crate chrono;
extern crate embedded_graphics;
extern crate rppal; // Crate for SPI, I2C, and GPIO on raspberry pi
extern crate ssd1306; // Crate for current I2C oled display

use chrono::{DateTime, Local};
use embedded_graphics::{
    fonts::{Font12x16, Text},
    pixelcolor::BinaryColor,
    prelude::*,
    style::TextStyleBuilder,
};
use rppal::i2c::I2c;
use ssd1306::{prelude::*, Builder, I2CDIBuilder};

/// Structure that contains screen information
pub struct ScreenDisplay {
    display: GraphicsMode<I2CInterface<I2c>>,
    // the closest train time
    train1: Option<DateTime<Local>>,
    // the second closest train time
    train2: Option<DateTime<Local>>,
}

// functions to initialize and change screen display
impl ScreenDisplay {
    /// Initializes a new screen display with empty train times
    pub fn new(address: u16) -> Result<ScreenDisplay, Box<dyn std::error::Error>> {
        // bus4 I2c connection information
        let mut i2c = I2c::new()?;
        i2c.set_slave_address(address)?;
        // creates an interface that connects to I2c
        let interface = I2CDIBuilder::new().init(i2c);
        // creates a new display connected to the interfce
        let mut disp: GraphicsMode<_> = Builder::new().connect(interface).into();
        // initializes the display
        disp.init().unwrap();
        Ok(ScreenDisplay {
            display: disp,
            train1: None,
            train2: None,
        })
    }

    /// Displays train1 and train2 on the screen display
    pub fn display_trains(
        &mut self,
        train_times: &[DateTime<Local>],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // create a variable to test whether or not the screen needs to be updated
        let mut update_screen = false;
        // if train1 is different than nearest train, replace with nearest train and update later
        if Some(train_times[0]) != self.train1 {
            self.train1 = Some(train_times[0]);
            update_screen = true;
        }
        // if there is more than one train time, proceed with train2
        if train_times.len() > 1 {
            // if train 2 is different from second train, replace and update
            if Some(train_times[1]) != self.train2 {
                self.train2 = Some(train_times[1]);
                update_screen = true;
            }
        } else {
            // if there is not more than one train time, set train2 as none
            self.train2 = None
        }
        // if train times were different than what's on the display, update display
        if update_screen {
            self.clear_display(false)?;
            // create a new text style for the screen with chosen font
            let text_style = TextStyleBuilder::new(Font12x16)
                .text_color(BinaryColor::On)
                .build();
            // if there is a train1, display train time
            if let Some(train1) = self.train1 {
                let time = train1.format("%H:%M").to_string();
                // creates text buffer
                Text::new(&time, Point::new(35, 5))
                    .into_styled(text_style)
                    .draw(&mut self.display)
                    .unwrap();
                // displays text buffer
                self.display.flush().unwrap();
            }
            // if there is a train2, display train time
            if let Some(train2) = self.train2 {
                let time = train2.format("%H:%M").to_string();
                // creats text buffer
                Text::new(&time, Point::new(35, 25))
                    .into_styled(text_style)
                    .draw(&mut self.display)
                    .unwrap();
                // displays text buffer
                self.display.flush().unwrap();
            }
        }
        Ok(())
    }

    /// Function to clear screen display
    pub fn clear_display(&mut self, reset_trains: bool) -> Result<(), Box<dyn std::error::Error>> {
        if reset_trains {
            self.train1 = None;
            self.train2 = None;
        }
        // clears the buffer
        self.display.clear();
        // sends cleared buffer to screen to refresh
        self.display.flush().unwrap();
        Ok(())
    }
}
