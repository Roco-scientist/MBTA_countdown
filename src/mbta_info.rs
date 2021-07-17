use scraper::{Html, Selector};
use std::{io::{BufWriter, BufReader},path::Path, fs::File, collections::HashMap};
use reqwest;
use serde_json;
use rayon::{ThreadPoolBuilder, prelude::*};
use std::sync::{Arc, Mutex};

/// Scrapes MBTA station and vehicle info from their website then stores the information in JSON files and returns in HashMaps
///
/// # Arguments
///
///  * `update` - a boolean on whether or not to force an update from the MBTA website
///
///  # Examples
///
///  ```
///  use mbta_countdown::mbta_info::all_mbta_info
///  let (vehicle_info, station_info) = all_mbta_info()?
///  ```
pub fn all_mbta_info(update: bool) -> Result<(HashMap<String, HashMap<String, String>>, HashMap<String, HashMap<String, Vec<String>>>), Box<dyn std::error::Error>> {
    // setup file names of the JSON files for saving or loading
    let mbta_vehicle_file_loc = "mbta_vehicle_info.json";
    let mbta_station_file_loc = "mbta_station_info.json";

    // if mbta vehicle JSON exists or update not called, read the JSON
    if !Path::new(&mbta_vehicle_file_loc).exists() | update {
        println!("Updating vehicle information");

        let vehicle_info_mutex = Arc::new(Mutex::new(HashMap::new()));
        let vehicle_clone_outer = Arc::clone(&vehicle_info_mutex);
        let pool = ThreadPoolBuilder::new().num_threads(3).build().expect("Threadpool failure");
        pool.scope(move |s|{
            let vehicle_clone_1 = Arc::clone(&vehicle_clone_outer);
            s.spawn(move |_s|{
                let commuter_info = retrieve_commuter().unwrap_or_else(|err| panic!("Error: {}", err));
                let mut vehicle_info_unlocked = vehicle_clone_1.lock().unwrap();
                vehicle_info_unlocked.insert("Commuter_Rail".to_string(), commuter_info);
            });
            let vehicle_clone_2 = Arc::clone(&vehicle_clone_outer);
            s.spawn(move |_s|{
                let subway_info = retrieve_subway().unwrap_or_else(|err| panic!("Error: {}", err));
                let mut vehicle_info_unlocked = vehicle_clone_2.lock().unwrap();
                vehicle_info_unlocked.insert("Subway".to_string(), subway_info);
            });
            let vehicle_clone_3 = Arc::clone(&vehicle_clone_outer);
            s.spawn(move |_s|{
                let ferry_info = retrieve_ferry().unwrap_or_else(|err| panic!("Error: {}", err));
                let mut vehicle_info_unlocked = vehicle_clone_3.lock().unwrap();
                vehicle_info_unlocked.insert("Ferry".to_string(), ferry_info);
            });
        });

        let f = File::create(&mbta_vehicle_file_loc)?;
        let bw = BufWriter::new(f);
        let vehicle_info = Arc::try_unwrap(vehicle_info_mutex).unwrap().into_inner()?;
        serde_json::to_writer(bw, &vehicle_info)?;
    }else{println!("Using existing vehicle information")};
    let g = File::open(&mbta_vehicle_file_loc)?;
    let reader = BufReader::new(g);
    let vehicle_info = serde_json::from_reader(reader)?;

    // if mbta station JSON exists or update not called, read the JSON
    if !Path::new(&mbta_station_file_loc).exists() | update {
        println!("Updating station information");
        // otherwise scrape all data from the website
        let station_info_to_write = retrieve_stations()?;
        let f = File::create(&mbta_station_file_loc)?;
        let bw = BufWriter::new(f);
        serde_json::to_writer(bw, &station_info_to_write)?;
    }else{println!("Using existing station information")};
    let g = File::open(&mbta_station_file_loc)?;
    let reader = BufReader::new(g);
    let station_info = serde_json::from_reader(reader)?;
    return Ok((vehicle_info, station_info))
}

/// Scrapes all station information from the MBTA websites and returns a HashMap of the information
fn retrieve_stations() -> Result<HashMap<String, HashMap<String, Vec<String>>>, Box<dyn std::error::Error>> {
    // Setup the urls for subway, commueter rail, and ferry
    let subway_url = "https://www.mbta.com/stops/subway#subway-tab";
    let communter_url = "https://www.mbta.com/stops/commuter-rail#commuter-rail-tab";
    let ferry_url = "https://www.mbta.com/stops/ferry#ferry-tab";

    // Parse the urls for the station information and add to the hashmap
    let mut station_conversion = HashMap::new();
    station_conversion = update_station_hashmap(station_conversion, parse_stations(subway_url)?);
    station_conversion = update_station_hashmap(station_conversion, parse_stations(communter_url)?);
    station_conversion = update_station_hashmap(station_conversion, parse_stations(ferry_url)?);
    return Ok(station_conversion)
}

fn update_station_hashmap(mut station_conversion: HashMap<String, HashMap<String, Vec<String>>>, new_stations_info: Vec<(String, String, Vec<String>)>) -> HashMap<String, HashMap<String, Vec<String>>> {
    for (station, station_api, vehicles) in new_stations_info {
        let mut api_veh = HashMap::new();
        api_veh.insert(station_api, vehicles);
        station_conversion.insert(station, api_veh);
    }
    return station_conversion
}

/// Pulls the station information along with vehicles that stop at the station from the given URL
fn parse_stations(url: &str) -> Result<Vec<(String, String, Vec<String>)>, Box<dyn std::error::Error>> {
    // get the website text
    let website_text = reqwest::blocking::get(url)?.text()?;

    // find all relevent buttons that contain the station information
    let document = Html::parse_document(&website_text);
    let button_selector = Selector::parse(r#"a[class="btn button stop-btn m-detailed-stop"]"#).unwrap();
    let buttons = document.select(&button_selector);

    // iterate on buttons and pull out the station information
    // Rayon threads connot pass iterated buttons, so this is done beforehand
    let station_api: Vec<(String, String)> = buttons.map(|button|{
        let station_name = button
            .value()
            .attr("data-name")
            .unwrap()
            .replace(" ", "_")
            .replace("'", "");
        let station_api_name = button
                .value()
                .attr("href")
                .unwrap()
                .replace("/stops/", "");
        (station_name, station_api_name) 
    }).collect();

    // create new vector to put parallel results into
    let mut station_info_all = Vec::new();

    // add station vehicles through rayon threads with par_iter
    station_api
        .par_iter()
        .map(|(station_name, station_api_name)| (station_name.clone(), station_api_name.clone(), station_vehicles(&station_api_name).unwrap_or_else(|err| panic!("Station vehicle error: {}", err))))
        .collect_into_vec(&mut station_info_all);

    return Ok(station_info_all)
}

/// Finds all vehicles that stop at the station of interest
fn station_vehicles(station_code: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    println!("Retrieving info for station: {}", station_code);
    // get the website text for the station
    let station_url = format!("https://www.mbta.com/stops/{}", station_code);
    let website_text = reqwest::blocking::get(&station_url)?.text()?;

    // parse and find the buttons which contain the vehicles that stop at the station
    let document = Html::parse_document(&website_text);
    let button_selector = Selector::parse(r#"a[class="c-link-block__outer-link"]"#).unwrap();
    let vehicle_buttons = document.select(&button_selector);

    // pull out vehicle codes from the buttons and place into a vec
    let vehicles: Vec<String> = vehicle_buttons.map(|button| button.value().attr("href").unwrap().replace("/schedules/", "")).collect();
    return Ok(vehicles)
}

/// Retrieve commuter rail conversion for MBTA API from common understandable name to MTBA API code
fn retrieve_commuter() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    println!("Starting Commuter");
    // use the commuter rail schedule website to find the commuter rail codes which are located within the buttons
    let commuter_url = "https://www.mbta.com/schedules/commuter-rail";
    // parse the commuter rail schedule website
    let commuter_info = parse_schedule_website(commuter_url, r#"a[class="c-grid-button c-grid-button--commuter-rail"]"#, r#"span[class="c-grid-button__name"]"#)?;
    // crate a hashmap out of the conversion information
    let commuter_conversion: HashMap<String, String> = commuter_info.iter().map(|commuter| (commuter[0].clone(), commuter[1].clone())).collect();
    println!("Finished Commuter");
    return Ok(commuter_conversion)
}

/// Retrieve ferry conversion for MBTA API from common understandable name to MTBA API code
fn retrieve_ferry() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    println!("Starting Ferry");
    // use the ferry schedule website to find the ferry codes which are located within the buttons
    let ferry_url = "https://www.mbta.com/schedules/ferry";
    // parse the ferry schedule website
    let ferry_info = parse_schedule_website(ferry_url, r#"a[class="c-grid-button c-grid-button--ferry"]"#, r#"span[class="c-grid-button__name"]"#)?;
    // crate a hashmap out of the conversion information
    let ferry_conversion: HashMap<String, String> = ferry_info.iter().map(|ferry| (ferry[0].clone(), ferry[1].clone())).collect();
    println!("Finished Ferry");
    return Ok(ferry_conversion)
}

/// Retrieve subway conversion for MBTA API from common understandable name to MTBA API code.
fn retrieve_subway() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    println!("Starting Subway");
    // use the subway schedule website to get the conversion information from the buttons
    let subway_url = "https://www.mbta.com/schedules/subway";
    // buttons are setup slightly different than the commuter rail.  Each colored line starts with the &str below but finishes with the color, so each needs to be determined for a scraper selector
    let button_selectors = partial_selector_match(subway_url, "c-grid-button c-grid-button--")?;
    // parse all button selectors and find subway conversion information.  Place into a hashmap
    let mut subway_info;
    let mut subway_conversion = HashMap::new();
    for button_selector in button_selectors.iter(){
        subway_info = parse_schedule_website(subway_url, &format!("a[class=\"{}\"]", button_selector), r#"span[class="c-grid-button__name"]"#)?;
        subway_conversion.extend(subway_info.iter().map(|subway| (subway[0].clone(), subway[1].clone())));
    }
    // green line buttons are condensed for each line.  This parses for these buttons
    let green_lines_info = parse_schedule_website(subway_url, r#"a[class="c-grid-button__condensed"]"#, r#"svg[role="img"]"#)?;
    // add green lines to the subway hashmap
    subway_conversion.extend(green_lines_info.iter().map(|green| (green[1].clone(), green[1].clone())));
    println!("Finished Subway");
    return Ok(subway_conversion)
}

/// Use to find all matches within an HTML when only a part is known.  For example here, when all subway button selectors start with 'c-grid-button--' but end with a different word.  This will find all occurances
fn partial_selector_match(url: &str, partial_match: &str) -> Result<Vec<String>, Box<dyn std::error::Error>>{
    // Get all website text
    let website_text = reqwest::blocking::get(url)?.text()?;
    // find any line that contains the partial match
    let selected_lines = website_text
        .lines()
        .filter(|website_line| website_line
            .contains(partial_match))
        // splie out the line and only take the text within the quotes with the match.  This is the exact match for the scraper selector
        .map(|website_line| website_line
            .split('"')
            .find(|inner_line| inner_line.contains(partial_match)).unwrap().to_string())
        .collect();
    return Ok(selected_lines)
}

/// Parses the schedule website.  This website is used to pull all vehicle information
fn parse_schedule_website(url: &str, button_select_str: &str, inner_select_str: &str) -> Result<Vec<[String; 2]>, Box<dyn std::error::Error>> {
    // get website text and parse
    let website_text = reqwest::blocking::get(url)?.text()?;
    let document = Html::parse_document(&website_text);

    // setup button selector and the inner HTML selector for the commonly known name for the train/subway
    let button_selector = Selector::parse(button_select_str).unwrap();
    let inner_selector = Selector::parse(inner_select_str).unwrap();

    // iteratively go through each button and return the line and the href, which contains the code for the line used by the API
    let button_selected = document.select(&button_selector);
    let conversion_info: Vec<[String; 2]> = button_selected
        .map(|button| [
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
                    ]
                ).collect();
    return Ok(conversion_info)
}

