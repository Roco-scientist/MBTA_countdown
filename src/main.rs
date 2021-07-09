extern crate rppal;
extern crate std;
use clap::{Arg, App};
use scraper::{Html, Selector};
use std::collections::HashMap;
use reqwest;

use MBTA_countdown;
// use rppal::gpio;
use std::{
    sync::{Arc, Mutex},
    thread, time,
};

fn main() {
    let (dir_code, station, clock_brightness, commuter_rail) = arguments().unwrap_or_else(|err| panic!("ERROR - train_times - {}", err));
    let minimum_display_min = 5i64;
    // get the initial time trains and put them in a thread safe value to be passed back and forth
    // between threads
    let train_times_option = Arc::new(Mutex::new(
        MBTA_countdown::train_time::train_times(&dir_code, &station, &commuter_rail)
            .unwrap_or_else(|err| panic!("ERROR - train_times - {}", err)),
    ));
    // create a new clock struct, this initializes the display
    let mut clock = MBTA_countdown::ht16k33_clock::ClockDisplay::new(0x70, clock_brightness)
        .unwrap_or_else(|err| panic!("ERROR - ClockDisplay - {}", err));
    // create a new screen struct, this initializes the display
    let mut screen = MBTA_countdown::ssd1306_screen::ScreenDisplay::new(0x3c)
        .unwrap_or_else(|err| panic!("ERROR - ScreenDisplay - {}", err));
    // clone the train_times to pass into thread
    let train_times_clone = Arc::clone(&train_times_option);
    // In a new thread find train times every minute and replace train_times with new value
    thread::spawn(move || loop {
        thread::sleep(time::Duration::from_secs(60));
        let new_train_times = MBTA_countdown::train_time::train_times(&dir_code, &station, &commuter_rail)
            .unwrap_or_else(|err| panic!("ERROR - train_times - {}", err));
        let mut old_train = train_times_clone.lock().unwrap();
        *old_train = new_train_times;
    });
    // continually update screen and clock every 0.25 seconds
    loop {
        thread::sleep(time::Duration::from_millis(250));
        // access and lock train times
        let train_times_unlocked = train_times_option.lock().unwrap();
        // if there are some train times, display on clock and screen
        if let Some(train_times) = &*train_times_unlocked {
            screen
                .display_trains(&train_times)
                .unwrap_or_else(|err| panic!("ERROR - display_trains - {}", err));
            clock
                .display_time_until(&train_times, &minimum_display_min)
                .unwrap_or_else(|err| panic!("ERROR - display_time_until - {}", err));
        } else {
            // if there are no train times, clear both displays
            screen
                .clear_display(true)
                .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
            clock
                .clear_display()
                .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
        }
    }
}

/// Gets the command line arguments
pub fn arguments() -> Result<(String, String, u8), Box<dyn std::error::Error>> {
    // let stations: HashMap<&str, &str> = [("South_Station", "sstat"), ("Forest_Hills", "forhl")].iter().cloned().collect();
    let stations = station_hasmap()?;
    let mut input_stations: Vec<&str> = stations.keys().map(|key| key.as_str()).collect();
    let commuter_rails = commuter_hasmap()?;
    let mut input_commuter: Vec<&str> = commuter_rails.keys().map(|key| key.as_str()).collect();
    input_commuter.sort();
    let args = App::new("MBTA train departure display")
        .version("0.2.0")
        .author("Rory Coffey <coffeyrt@gmail.com>")
        .about("Displays the departure of the Needham MBTA commuter rail")
        .arg(
            Arg::with_name("direction")
                .short("d")
                .long("direction")
                .takes_value(true)
                .required(true)
                .possible_values(&["inbound", "outbound"])
                .help("Train direction"),
        )
        .arg(
            Arg::with_name("station")
                .short("s")
                .long("station")
                .takes_value(true)
                .required(true)
                .possible_values(&input_stations)
                .help("Train station.  Only setup for commuter rail right now"),
        )
        .arg(
            Arg::with_name("commuter_rail")
                .short("r")
                .long("commuter_rail")
                .takes_value(true)
                .required(true)
                .possible_values(&input_commuter)
                .help("Commuter rail line"),
        )
        .arg(
            Arg::with_name("clock_brightness")
                .short("c")
                .long("clock_brightness")
                .takes_value(true)
                .help("Scale to set clock brightness, 0-9"),
        )
        .get_matches();
    let mut dir_code = String::new();
    let mut station = String::new();
    let clock_brightness;
    // reforms direction input to the direction code used in the API
    if let Some(direction_input) = args.value_of("direction") {
        match direction_input{
            "inbound" => dir_code = "1".to_string(),
            "outbound" => dir_code = "0".to_string(),
            _ => panic!("Unknown direction input")
        }
    };
    if let Some(station_input) = args.value_of("station") {
        station = stations.get(station_input).unwrap().to_string();
    };
    if let Some(commuter_input) = args.value_of("commuter_rail") {
        commuter_rail = commuter_rail.get(commuter_input).unwrap().to_string();
    };
    if let Some(clock_bright_input) = args.value_of("clock_brightness") {
        clock_brightness = clock_bright_input.parse::<u8>()?;
    }else{
        clock_brightness = 7u8;
    };
    return Ok((dir_code, station, clock_brightness, commuter_rail));
}

fn station_hasmap() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let subway_url = "https://www.mbta.com/stops/subway#subway-tab";
    let communter_url = "https://www.mbta.com/stops/commuter-rail#commuter-rail-tab";
    let ferry_url = "https://www.mbta.com/stops/ferry#ferry-tab";
    let stations_info = get_stations(subway_url)?;
    let mut station_conversion: HashMap<String, String> = stations_info.iter().cloned().collect();
    station_conversion.extend(get_stations(communter_url)?);
    station_conversion.extend(get_stations(ferry_url)?);
    return Ok(station_conversion)
}

fn get_stations(url: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let website_text = reqwest::blocking::get(url)?.text()?;
    let document = Html::parse_document(&website_text);
    let selector = Selector::parse(r#"a[class="btn button stop-btn m-detailed-stop"]"#).unwrap();
    let station_select = document.select(&selector);
    let station_conversion: Vec<(String, String)> = station_select
        .map(|button| (
                button
                .value()
                .attr("data-name")
                .unwrap().replace(" ", "_")
                .replace("'", ""), 
                button
                .value()
                .attr("href")
                .unwrap()
                .split("stop/")
                .last()
                .unwrap()
                .to_string()
                )
            )
        .collect();
    return Ok(station_conversion)
}

fn commuter_hasmap() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let commuter_url = "https://www.mbta.com/schedules/commuter-rail";
    let commuter_info = get_commuter(commuter_url)?;
    let commuter_conversion: HashMap<String, String> = commuter_info.iter().cloned().collect();
    return Ok(commuter_conversion)
}

fn get_commuter(url: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let website_text = reqwest::blocking::get(url)?.text()?;
    let document = Html::parse_document(&website_text);
    let button_selector = Selector::parse(r#"a[class="c-grid-button c-grid-button--commuter-rail"]"#).unwrap();
    let inner_selector = Selector::parse(r#"span[class="c-grid-button__name"]"#).unwrap();
    let commuter_select = document.select(&button_selector);
    let commuter_conversion: Vec<(String, String)> = commuter_select
        .map(|button| (
                button
                .select(&inner_selector)
                .last()
                .unwrap()
                .inner_html()
                .replace("\u{200b}", "")
                .replace("\n", "")
                .replace("Line", "")
                .trim()
                .replace(" ", "_")
                .replace("'", ""), 
                button
                .value()
                .attr("href")
                .unwrap()
                .replace("/schedules/", "")
                    )
                ).collect();
    return Ok(commuter_conversion)
}
