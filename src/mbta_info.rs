use scraper::{Html, Selector};
use std::{io::{BufWriter, BufReader},path::Path, fs::File, collections::HashMap};
use reqwest;
use serde_json;

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

    // initiate vehicle hashmap for inner scope
    let mut vehicle_info;
    // if mbta vehicle JSON exists or update not called, read the JSON
    if Path::new(&mbta_vehicle_file_loc).exists() && !update {
        let g = File::open(&mbta_vehicle_file_loc)?;
        let reader = BufReader::new(g);
        vehicle_info = serde_json::from_reader(reader)?;
    }else{
        // otherwise scrape all data from the website and save into JSON files
        let commuter_info = retrieve_commuter()?;
        let subway_info = retrieve_subway()?;
        let ferry_info = retrieve_ferry()?;
        vehicle_info = HashMap::new();
        vehicle_info.insert("Commuter_Rail".to_string(), commuter_info);
        vehicle_info.insert("Subway".to_string(), subway_info);
        vehicle_info.insert("Ferry".to_string(), ferry_info);
        let f = File::create(&mbta_vehicle_file_loc)?;
        let bw = BufWriter::new(f);
        serde_json::to_writer(bw, &vehicle_info)?;
    }

    // initiate station hashmap for inner scope
    let station_info;
    // if mbta station JSON exists or update not called, read the JSON
    if Path::new(&mbta_station_file_loc).exists() && !update {
        let g = File::open(&mbta_station_file_loc)?;
        let reader = BufReader::new(g);
        station_info = serde_json::from_reader(reader)?;
    }else{
        // otherwise scrape all data from the website
        station_info = retrieve_stations()?;
        let f = File::create(&mbta_station_file_loc)?;
        let bw = BufWriter::new(f);
        serde_json::to_writer(bw, &station_info)?;
    }
    return Ok((vehicle_info, station_info))

}

/// Scrapes all station information from the MBTA websites and returns a HashMap of the information
fn retrieve_stations() -> Result<HashMap<String, HashMap<String, Vec<String>>>, Box<dyn std::error::Error>> {
    // Setup the urls for subway, commueter rail, and ferry
    let subway_url = "https://www.mbta.com/stops/subway#subway-tab";
    let communter_url = "https://www.mbta.com/stops/commuter-rail#commuter-rail-tab";
    let ferry_url = "https://www.mbta.com/stops/ferry#ferry-tab";

    // Parse the urls for the station information and add to the hashmap
    let stations_info = parse_stations(subway_url)?;
    let mut station_conversion: HashMap<String, HashMap<String, Vec<String>>> = stations_info.iter().cloned().collect();
    station_conversion.extend(parse_stations(communter_url)?);
    station_conversion.extend(parse_stations(ferry_url)?);
    return Ok(station_conversion)
}

/// Pulls the station information along with vehicles that stop at the station from the given URL
fn parse_stations(url: &str) -> Result<Vec<(String, HashMap<String, Vec<String>>)>, Box<dyn std::error::Error>> {
    // get the website text
    let website_text = reqwest::blocking::get(url)?.text()?;

    // find all relevent buttons that contain the station information
    let document = Html::parse_document(&website_text);
    let button_selector = Selector::parse(r#"a[class="btn button stop-btn m-detailed-stop"]"#).unwrap();
    let buttons = document.select(&button_selector);

    // iterate on buttons and pull out the station information
    let station_conversion: Vec<(String, HashMap<String, Vec<String>>)> = buttons
        .map(|button| (
                // get and rename the common understood station name
                button
                .value()
                .attr("data-name")
                .unwrap()
                .replace(" ", "_")
                .replace("'", ""), 
                // get the API station name and the vehicles that stop at the station
                station_vehicles(
                    button
                    .value()
                    .attr("href")
                    .unwrap()
                    .replace("/stops/", "")
                    ).unwrap()
                )
            )
        .collect();
    return Ok(station_conversion)
}

/// Finds all vehicles that stop at the station of interest
fn station_vehicles(station_code: String) -> Result<HashMap<String, Vec<String>>, Box<dyn std::error::Error>> {
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

    // create hashmap of station_code:[vehicle_codes]
    let mut station_vehicles_hash = HashMap::new();
    station_vehicles_hash.insert(station_code, vehicles);
    return Ok(station_vehicles_hash)
}

/// Retrieve commuter rail conversion for MBTA API from common understandable name to MTBA API code
fn retrieve_commuter() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // use the commuter rail schedule website to find the commuter rail codes which are located within the buttons
    let commuter_url = "https://www.mbta.com/schedules/commuter-rail";
    // parse the commuter rail schedule website
    let commuter_info = parse_schedule_website(commuter_url, r#"a[class="c-grid-button c-grid-button--commuter-rail"]"#, r#"span[class="c-grid-button__name"]"#)?;
    // crate a hashmap out of the conversion information
    let commuter_conversion: HashMap<String, String> = commuter_info.iter().map(|commuter| (commuter[0].clone(), commuter[1].clone())).collect();
    return Ok(commuter_conversion)
}

/// Retrieve ferry conversion for MBTA API from common understandable name to MTBA API code
fn retrieve_ferry() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // use the ferry schedule website to find the ferry codes which are located within the buttons
    let ferry_url = "https://www.mbta.com/schedules/ferry";
    // parse the ferry schedule website
    let ferry_info = parse_schedule_website(ferry_url, r#"a[class="c-grid-button c-grid-button--ferry"]"#, r#"span[class="c-grid-button__name"]"#)?;
    // crate a hashmap out of the conversion information
    let ferry_conversion: HashMap<String, String> = ferry_info.iter().map(|ferry| (ferry[0].clone(), ferry[1].clone())).collect();
    return Ok(ferry_conversion)
}

/// Retrieve subway conversion for MBTA API from common understandable name to MTBA API code.
fn retrieve_subway() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
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

