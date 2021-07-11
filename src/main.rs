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
    let (dir_code, station, clock_brightness, vehicle_code) = arguments().unwrap_or_else(|err| panic!("ERROR - train_times - {}", err));
    let minimum_display_min = 5i64;
    // get the initial time trains and put them in a thread safe value to be passed back and forth
    // between threads
    let train_times_option = Arc::new(Mutex::new(
        MBTA_countdown::train_time::train_times(&dir_code, &station, &vehicle_code)
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
        let new_train_times = MBTA_countdown::train_time::train_times(&dir_code, &station, &vehicle_code)
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
    let (vehicle_info, station_info) = MBTA_countdown::mbta_info::all_mbta_info(false)?;
    let mut input_stations: Vec<&str> = station_info.keys().map(|key| key.as_str()).collect();
    input_stations.sort();
    let commuter_rails = vehicle_info.get("Commuter_Rail")?;
    let mut input_commuter: Vec<&str> = commuter_rails.keys().map(|key| key.as_str()).collect();
    input_commuter.sort();
    let subway_lines = vehicle_info.get("Subway")?;
    let mut input_subway: Vec<&str> = subway_lines.keys().map(|key| key.as_str()).collect();
    input_subway.sort();
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
                .required_unless("update_mbta")
                .possible_values(&input_stations)
                .help("Train station.  Only setup for commuter rail right now"),
        )
        .arg(
            Arg::with_name("commuter_rail")
                .short("r")
                .long("commuter_rail")
                .takes_value(true)
                .required_unless_one(&["subway_line", "update_mbta"])
                .possible_values(&input_commuter)
                .help("Commuter rail line"),
        )
        .arg(
            Arg::with_name("subway_line")
                .short("l")
                .long("subway_line")
                .takes_value(true)
                .required_unless_one(&["commuter_rail", "update_mbta"])
                .possible_values(&input_subway)
                .help("Subway line"),
        )
        .arg(
            Arg::with_name("clock_brightness")
                .short("c")
                .long("clock_brightness")
                .takes_value(true)
                .help("Scale to set clock brightness, 0-9"),
        )
        .arg(
            Arg::with_name("update_mbta")
                .short("u")
                .long("update_mbta")
                .takes_value(false)
                .help("Update MBTA info from their website"),
        )
        .get_matches();
    if args.is_present("update_mbta") {
        println!("Updating MBTA info");
        MBTA_countdown::mbta_info::all_mbta_info(true);
        println!("Finished updating MBTA info");
        std::process::exit(0i32);
    }
    let mut dir_code;
    let mut station;
    let mut vehicle_code;
    // reforms direction input to the direction code used in the API
    if let Some(direction_input) = args.value_of("direction") {
        match direction_input{
            "inbound" => dir_code = "1".to_string(),
            "outbound" => dir_code = "0".to_string(),
            _ => panic!("Unknown direction input")
        }
    };
    if let Some(commuter_input) = args.value_of("commuter_rail") {
        vehicle_code = commuter_rail.get(commuter_input).unwrap().to_string();
    }else{
        if let Some(subway) = args.value_of("subway_line") {
            vehicle_code = subway_lines.get(subway).unwrap().to_string();
        }
    };
    if let Some(station_input) = args.value_of("station") {
        let station_hashmap = station_info.get(station_input).unwrap();
        station = station_hashmap.keys().last().unwrap().to_string();
        let stopping = station_hashmap.get(&station).unwrap();
        if !stopping.contains(&vehicle_code){
            panic!("{} not at {}\nStopping at {}: {:?}", vehicle_code, station, station, stopping)
        }
    };
    let clock_brightness = args.value_of("clock_brightness").unwrap_or("7").parse::<u8>()?;
    return Ok((dir_code, station, clock_brightness, vehicle_code));
}
