use clap::{Arg, App};
use mbta_countdown;
use std;
use std::{
    sync::{Arc, Mutex},
    thread, time,collections::HashMap,
    io::{Read, Write, stdout},
};
use termion;
use termion::{async_stdin, raw::IntoRawMode};

fn main() {
    let (dir_code, station, clock_brightness, vehicle_code) = arguments().unwrap_or_else(|err| panic!("ERROR - train_times - {}", err));
    let minimum_display_min = 5i64;
    // get the initial time trains and put them in a thread safe value to be passed back and forth
    // between threads
    let train_times_option = Arc::new(Mutex::new(
        mbta_countdown::train_time::train_times(&dir_code, &station, &vehicle_code)
            .unwrap_or_else(|err| panic!("ERROR - train_times - {}", err)),
    ));
    // create a new clock struct, this initializes the display
    let mut clock = mbta_countdown::ht16k33_clock::ClockDisplay::new(0x70, clock_brightness)
        .unwrap_or_else(|err| panic!("ERROR - ClockDisplay - {}", err));
    // create a new screen struct, this initializes the display
    let mut screen = mbta_countdown::ssd1306_screen::ScreenDisplay::new(0x3c)
        .unwrap_or_else(|err| panic!("ERROR - ScreenDisplay - {}", err));
    // clone the train_times to pass into thread
    let train_times_clone = Arc::clone(&train_times_option);
    // set quit to false to have a clean quit
    let quit = Arc::new(Mutex::new(false));
    let quit_clone = Arc::clone(&quit);
    // In a new thread find train times every minute and replace train_times with new value
    let train_time_thread = thread::spawn(move || loop {
        thread::sleep(time::Duration::from_secs(60));
        let new_train_times = mbta_countdown::train_time::train_times(&dir_code, &station, &vehicle_code)
            .unwrap_or_else(|err| panic!("ERROR - train_times - {}", err));
        let mut old_train = train_times_clone.lock().unwrap();
        *old_train = new_train_times;
        let quit_unlocked = quit_clone.lock().unwrap();
        if *quit_unlocked {break};
    });

    let stdout = stdout();
    let mut stdout = stdout.lock().into_raw_mode().unwrap();
    let mut stdin = async_stdin().bytes();

    // setup the screen as blank with 'q to quit'
    write!(stdout,
           "{}{}",
           termion::clear::All,
           termion::cursor::Goto(1, 1))
            .unwrap();

    write!(stdout, "{}", termion::clear::CurrentLine).unwrap();
    write!(stdout, "{}{}q{}{} to quit", 
        termion::color::Fg(termion::color::Green),
        termion::style::Bold,
        termion::color::Fg(termion::color::Reset),
        termion::style::NoBold).unwrap();

    write!(stdout,
           "{}{}",
           termion::cursor::Goto(1, 2), termion::cursor::Hide)
            .unwrap();

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

        // get key input and quit if q is pressed with async_stdin
        let key_input = stdin.next();
        match key_input {
            Some(Ok(b'q')) => break,
            Some(a) => write!(stdout, "{}\r{}",termion::clear::CurrentLine, a.unwrap() as char).unwrap(),
            _ => (),
        }
        stdout.flush().unwrap();
    }
    write!(stdout, "{}{}Cleaning up and quiting.  May take up to a minute",
        termion::clear::All,
        termion::cursor::Show).unwrap();
    screen.clear_display(true).unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
    clock.clear_display().unwrap_or_else(|err| panic!("ERROR - clear_display - {}", err));
    let mut quit_unlocked = quit.lock().unwrap();
    *quit_unlocked = true;
    train_time_thread.join().unwrap_or_else(|err| panic!("ERROR - clear_display - {:?}", err));
}

/// Gets the command line arguments
pub fn arguments() -> Result<(String, String, u8, String), Box<dyn std::error::Error>> {
    // get station and vehicle conversions for the MBTA API
    let (vehicle_info, station_info) = mbta_countdown::mbta_info::all_mbta_info(false)?;
    // get a list of stations to limit the station argument input
    let mut input_stations: Vec<&str> = station_info.keys().map(|key| key.as_str()).collect();
    input_stations.sort();
    // create an empty hashmap to handle errors when the key does not exist and update is called
    let mut empty_vehicle_hashmap = HashMap::new();
    empty_vehicle_hashmap.insert("".to_string(), "".to_string());
    // get a list of commuter rail lines to limit the commuter rail argument input
    let commuter_rails = vehicle_info.get("Commuter_Rail").unwrap_or(&empty_vehicle_hashmap);
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
        .version("0.3.1")
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
                .required_unless_one(&["commuter_rail",  "ferry_line", "update_mbta"])
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
        std::process::exit(0i32);
    }

    // reforms direction input to the direction code used in the API
    let mut dir_code = String::new();
    if let Some(direction_input) = args.value_of("direction") {
        match direction_input{
            "inbound" => dir_code = "1".to_string(),
            "outbound" => dir_code = "0".to_string(),
            _ => panic!("Unknown direction input")
        }
    };

    // Convert either commuter_rail or subway_line to MBTA API vehicle code
    let mut vehicle_code = String::new();
    if let Some(commuter_input) = args.value_of("commuter_rail") {
        vehicle_code = commuter_rails.get(commuter_input).unwrap().to_owned();
    }else{
        if let Some(subway) = args.value_of("subway_line") {
            vehicle_code = subway_lines.get(subway).unwrap().to_owned();
        }else{
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
        if !stopping.contains(&vehicle_code){
            panic!("{} not at {}\nStopping at {}: {:?}", vehicle_code, station, station, stopping)
        }
    };

    // either set clock_brightness to input or defaul to 7
    let clock_brightness = args.value_of("clock_brightness").unwrap().parse::<u8>()?;
    return Ok((dir_code, station, clock_brightness, vehicle_code));
}
