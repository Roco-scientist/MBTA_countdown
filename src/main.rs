use chrono::prelude::*;
use chrono::Local;
use clap::{App, Arg};
use mbta_countdown;
use rppal::gpio;
use std;
use std::{
    collections::HashMap,
    io::{stdout, Read, Write},
    process::{exit, Command},
    sync::{Arc, Mutex},
    time::Duration,
};
use termion;
use termion::{async_stdin, raw::IntoRawMode};

#[tokio::main]
async fn main() {
    let (dir_code, station, clock_brightness, vehicle_code, clock_type) =
        arguments().unwrap_or_else(|err| panic!("ERROR - train_times - {}", err));
    let minimum_display_min = 5i64;

    // setup the screen as blank with 'q to quit'
    let out = stdout();
    let mut stdout_main = out.lock().into_raw_mode().unwrap();
    let mut stdin = async_stdin().bytes();

    write!(
        stdout_main,
        "{}{}{}",
        termion::clear::All,
        termion::cursor::Goto(1, 1),
        termion::cursor::Hide
    )
    .unwrap();
    write!(
        stdout_main,
        "{}{}q{}{} to quit",
        termion::color::Fg(termion::color::Green),
        termion::style::Bold,
        termion::color::Fg(termion::color::Reset),
        termion::style::NoBold
    )
    .unwrap();
    stdout_main.flush().unwrap();

    // setup variables that are passed between threads
    let quit = Arc::new(Mutex::new(false)); // set quit to false to have a clean quit
    let shutdown = Arc::new(Mutex::new(false)); // shutdown varaible passed after button press

    // clone quit and shutdown variable to put into async thread for button
    let quit_clone = Arc::clone(&quit);
    let shutdown_clone = Arc::clone(&shutdown);

    // setup async interrup which shuts down the device if bthe button is pressed
    let gpio = gpio::Gpio::new().unwrap_or_else(|err| panic!("ERROR - gpio - {}", err));
    let mut shutdown_pin = gpio
        .get(13)
        .unwrap_or_else(|err| panic!("ERROR - pin - {}", err))
        .into_input_pulldown();
    shutdown_pin
        .set_async_interrupt(gpio::Trigger::RisingEdge, move |_| {
            *quit_clone.lock().unwrap() = true;
            *shutdown_clone.lock().unwrap() = true;
        })
        .unwrap();

    // setup countdown clock.  If the type is not TM1637, the I2C address is used, otherwise it is
    // bit banged
    let address;
    if clock_type == "TM1637".to_string() {
        address = None;
    } else {
        address = Some(0x70)
    }
    // Initiate the countdown clock
    let mut clock;
    clock = mbta_countdown::clocks::Clocks::new(clock_type, clock_brightness, address)
        .unwrap_or_else(|err| panic!("ERROR - clock - {}", err));

    // Get the scheduled and predicted train times to display and countdown from
    let train_times = Arc::new(Mutex::new(
        mbta_countdown::train_time::train_times(&dir_code, &station, &vehicle_code)
            .unwrap_or_else(|err| panic!("ERROR - train_times - {}", err)),
    ));

    let train_times_clone = Arc::clone(&train_times);
    let quit_clone = Arc::clone(&quit);

    let pause_overnight = Arc::new(Mutex::new(false));
    let pause_overnight_clone = Arc::clone(&pause_overnight);

    let screen_train_thread = tokio::spawn(async move {
        let mut train_time_errors = 0u8;
        let mut screen = mbta_countdown::ssd1306_screen::ScreenDisplay::new(0x3c)
            .unwrap_or_else(|err| panic!("ERROR - ScreenDisplay - {}", err));

        // get the first and last train for the day to know when to pause the displays and not
        // continually update when there are no trains arriving
        let last_first =
            mbta_countdown::train_time::max_min_times(&dir_code, &station, &vehicle_code)
                .unwrap_or_else(|err| panic!("Error - max min times - {}", err));
        let mut last_time;
        let mut first_time;
        if let Some([last, _]) = last_first {
            last_time = last;
            // println!("First: {}\nLast: {}", first_time, last_time);
        } else {
            panic!("Missing vehicles")
        };

        loop {
            // get the current time
            let mut now = Local::now();

            // if the current time is after the last time start a pause
            if now > last_time {
                println!("Last: {}", last_time);
                *pause_overnight_clone.lock().unwrap() = true;

                // if it is less than 3 am or the same day of the last vehicle, pause for 5 minutes
                // and recheck the time
                while now.hour() < 3 || now.hour() == 24 || now.day() == last_time.day() {
                    if *quit_clone.lock().unwrap() {
                        break;
                    };
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    now = Local::now();
                }

                // after 3 am get the first and last vehicle times
                let last_first_thread =
                    mbta_countdown::train_time::max_min_times(&dir_code, &station, &vehicle_code)
                        .unwrap_or_else(|err| panic!("Error - max min times - {}", err));
                if let Some([last, first]) = last_first_thread {
                    last_time = last;
                    first_time = first;
                    println!("First: {}", first_time);
                } else {
                    panic!("Missing vehicles")
                };

                // get the hour one earlier than the first train to begin the countdown again
                let one_hour = first_time.hour() - 1;
                // Pause until one hour before the first train
                while now.hour() < one_hour {
                    if *quit_clone.lock().unwrap() {
                        break;
                    };
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    now = Local::now();
                }

                // after all puases are done, return false to the other thread to allow it to
                // continue
                *pause_overnight_clone.lock().unwrap() = false;
            };

            // if any other thread changes the quit variable to true, break out of the loop and
            // clear the display screen
            if *quit_clone.lock().unwrap() {
                screen
                    .clear_display(true)
                    .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
                break;
            };

            // if there are train times, display them on the screen, otherwise clear the display
            if let Some(ref train_times_list) = *train_times_clone.lock().unwrap() {
                screen
                    .display_trains(&train_times_list)
                    .unwrap_or_else(|err| panic!("ERROR - display_trains - {}", err));
            } else {
                screen
                    .clear_display(true)
                    .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
            };

            // async pause for 120 seconds donw in single seconds for a clean quit
            for _ in 0..120u8 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if *quit_clone.lock().unwrap() {
                    screen
                        .clear_display(true)
                        .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
                    break;
                };
            }

            // If there is no error on retrieving the train times from the website, update the
            // train_times variable, otherwise allow up to 5 errors
            if let Ok(new_train_times) =
                mbta_countdown::train_time::train_times(&dir_code, &station, &vehicle_code)
            {
                *train_times_clone.lock().unwrap() = new_train_times;
                train_time_errors = 0;
            } else {
                train_time_errors += 1;
                if train_time_errors == 5 {
                    panic!("Unable to retrieve train times for 10 minutes");
                }
            }
        }
    });

    // start the loop for the countdown clock
    loop {
        // if display thread declares a pause, pause the countdown for 5 minutes
        while *pause_overnight.lock().unwrap() {
            tokio::time::sleep(Duration::from_secs(300)).await;
            // write!(stdout_main, "{}Paused", termion::cursor::Goto(1, 3),).unwrap();
            let key_input = stdin.next();
            match key_input {
                Some(Ok(b'q')) => {
                    *quit.lock().unwrap() = true;
                    break;
                }
                Some(a) => {
                    write!(
                        stdout_main,
                        "{}{}",
                        termion::cursor::Goto(2, 1),
                        a.unwrap() as char
                    )
                    .unwrap();
                    stdout_main.flush().unwrap();
                }
                _ => (),
            };
            println!("Paused");
        }
        write!(stdout_main, "{}      ", termion::cursor::Goto(1, 3),).unwrap();
        if *quit.lock().unwrap() {
            break;
        };
        tokio::time::sleep(Duration::from_millis(250)).await;
        // if there are some train times, display on clock and screen
        if let Some(ref train_times_list) = *train_times.lock().unwrap() {
            clock
                .display_time_until(&train_times_list, &minimum_display_min)
                .unwrap_or_else(|err| panic!("ERROR - display_time_until - {}", err));
            let key_input = stdin.next();
            match key_input {
                Some(Ok(b'q')) => {
                    *quit.lock().unwrap() = true;
                    break;
                }
                Some(a) => {
                    write!(
                        stdout_main,
                        "{}{}",
                        termion::cursor::Goto(2, 1),
                        a.unwrap() as char
                    )
                    .unwrap();
                    stdout_main.flush().unwrap();
                }
                _ => (),
            };
        } else {
            clock
                .clear_display()
                .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
            let key_input = stdin.next();
            match key_input {
                Some(Ok(b'q')) => {
                    *quit.lock().unwrap() = true;
                    break;
                }
                Some(a) => {
                    write!(
                        stdout_main,
                        "{}{}",
                        termion::cursor::Goto(2, 1),
                        a.unwrap() as char
                    )
                    .unwrap();
                    stdout_main.flush().unwrap();
                }
                _ => (),
            };
        };
    }

    screen_train_thread
        .await
        .unwrap_or_else(|err| panic!("ERROR - train thread - {}", err));

    write!(
        stdout_main,
        "{}{}Finished",
        termion::cursor::Goto(1, 3),
        termion::cursor::Show
    )
    .unwrap();

    stdout_main.flush().unwrap();
    drop(stdout_main);
    println!();

    clock
        .clear_display()
        .unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));

    if *shutdown.lock().unwrap() {
        println!("Shutting down");
        Command::new("shutdown")
            .arg("-h")
            .arg("now")
            .output()
            .unwrap();
    }
}

/// Gets the command line arguments
pub fn arguments() -> Result<(String, String, u8, String, String), Box<dyn std::error::Error>> {
    // get station and vehicle conversions for the MBTA API
    let (vehicle_info, station_info) = mbta_countdown::mbta_info::all_mbta_info(false)?;
    // get a list of stations to limit the station argument input
    let mut input_stations: Vec<&str> = station_info.keys().map(|key| key.as_str()).collect();
    input_stations.sort();
    // create an empty hashmap to handle errors when the key does not exist and update is called
    let mut empty_vehicle_hashmap = HashMap::new();
    empty_vehicle_hashmap.insert("".to_string(), "".to_string());
    // get a list of commuter rail lines to limit the commuter rail argument input
    let commuter_rails = vehicle_info
        .get("Commuter_Rail")
        .unwrap_or(&empty_vehicle_hashmap);
    let mut input_commuter: Vec<&str> = commuter_rails.keys().map(|key| key.as_str()).collect();
    input_commuter.sort();
    // get a list of subway lines to limit the subway argument input
    let subway_lines = vehicle_info.get("Subway").unwrap_or(&empty_vehicle_hashmap);
    let mut input_subway: Vec<&str> = subway_lines.keys().map(|key| key.as_str()).collect();
    input_subway.sort();
    // get a list of ferry lines to limit the ferry argument input
    let ferry_lines = vehicle_info.get("Ferry").unwrap_or(&empty_vehicle_hashmap);
    let mut input_ferry: Vec<&str> = ferry_lines.keys().map(|key| key.as_str()).collect();
    input_ferry.sort();

    // parse arguments
    let args = App::new("MBTA train departure display")
        .version("0.3.2")
        .author("Rory Coffey <coffeyrt@gmail.com>")
        .about("Displays the departure of the Needham MBTA commuter rail")
        .arg(
            Arg::with_name("direction")
                .short("d")
                .long("direction")
                .takes_value(true)
                .required_unless("update_mbta")
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
                .help("Train station"),
        )
        .arg(
            Arg::with_name("commuter_rail")
                .short("c")
                .long("commuter_rail")
                .takes_value(true)
                .required_unless_one(&["subway_line", "ferry_line", "update_mbta"])
                .possible_values(&input_commuter)
                .help("Commuter rail line"),
        )
        .arg(
            Arg::with_name("subway_line")
                .short("l")
                .long("subway_line")
                .takes_value(true)
                .required_unless_one(&["commuter_rail", "ferry_line", "update_mbta"])
                .possible_values(&input_subway)
                .help("Subway line"),
        )
        .arg(
            Arg::with_name("ferry_line")
                .short("f")
                .long("ferry_line")
                .takes_value(true)
                .required_unless_one(&["commuter_rail", "subway_line", "update_mbta"])
                .possible_values(&input_ferry)
                .help("Ferry line"),
        )
        .arg(
            Arg::with_name("clock_brightness")
                .short("b")
                .long("clock_brightness")
                .takes_value(true)
                .default_value("7")
                .help("Scale to set clock brightness, 0-9"),
        )
        .arg(
            Arg::with_name("clock_type")
                .short("t")
                .long("clock_type")
                .takes_value(true)
                .default_value("HT16K33")
                .possible_values(&["HT16K33", "TM1637"])
                .help("Set countdown clock type"),
        )
        .arg(
            Arg::with_name("update_mbta")
                .short("u")
                .long("update_mbta")
                .takes_value(false)
                .help("Update MBTA info from their website"),
        )
        .get_matches();

    // if update_mbta is called, update mbta info then exit
    if args.is_present("update_mbta") {
        println!("Updating MBTA info");
        mbta_countdown::mbta_info::all_mbta_info(true)?;
        println!("Finished updating MBTA info");
        exit(0i32);
    }

    let clock_type = args.value_of("clock_type").unwrap().to_string();
    // let clock_type = match args.value_of("clock_type").unwrap() {
    //     "HT16K33" => ClockType::HT16K33,
    //     "TM1637" => ClockType::TM1637,
    //     _ => panic!("Unknown clock type")
    // };

    // reforms direction input to the direction code used in the API
    let mut dir_code = String::new();
    if let Some(direction_input) = args.value_of("direction") {
        match direction_input {
            "inbound" => dir_code = "1".to_string(),
            "outbound" => dir_code = "0".to_string(),
            _ => panic!("Unknown direction input"),
        }
    };

    // Convert either commuter_rail or subway_line to MBTA API vehicle code
    let mut vehicle_code = String::new();
    if let Some(commuter_input) = args.value_of("commuter_rail") {
        vehicle_code = commuter_rails.get(commuter_input).unwrap().to_owned();
    } else {
        if let Some(subway) = args.value_of("subway_line") {
            vehicle_code = subway_lines.get(subway).unwrap().to_owned();
        } else {
            if let Some(ferry) = args.value_of("ferry_line") {
                vehicle_code = ferry_lines.get(ferry).unwrap().to_owned()
            }
        }
    };

    // Convert station to API code and check if the vehicle code exists at the station
    let mut station = String::new();
    if let Some(station_input) = args.value_of("station") {
        let station_hashmap = station_info.get(station_input).unwrap();
        station = station_hashmap.keys().last().unwrap().to_owned();
        let stopping = station_hashmap.get(&station).unwrap();
        if !stopping.contains(&vehicle_code) {
            panic!(
                "{} not at {}\nStopping at {}: {:?}",
                vehicle_code, station, station, stopping
            )
        }
    };

    // either set clock_brightness to input or defaul to 7
    let clock_brightness = args.value_of("clock_brightness").unwrap().parse::<u8>()?;
    return Ok((
        dir_code,
        station,
        clock_brightness,
        vehicle_code,
        clock_type,
    ));
}
