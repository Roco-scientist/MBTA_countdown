use scraper::{Html, Selector};
use std::{io::{BufWriter, BufReader},path::Path, fs::File, collections::HashMap};
use reqwest;
use serde_json;

pub fn all_mbta_info(update: bool) -> Result<HashMap<String, HashMap<String, String>>, Box<dyn std::error::Error>> {
    let mbta_file_loc = "mbta_info.json";
    let mut all_info;
    if Path::new(&mbta_file_loc).exists() && !update {
        let g = File::open(&mbta_file_loc)?;
        let reader = BufReader::new(g);
        all_info = serde_json::from_reader(reader)?;
    }else{
        let commuter_info = retrieve_commuter()?;
        let station_info = retrieve_station()?;
        all_info = HashMap::new();
        all_info.insert("Commuter_Rail".to_string(), commuter_info);
        all_info.insert("Stations".to_string(), station_info);
        let f = File::create(&mbta_file_loc)?;
        let bw = BufWriter::new(f);
        serde_json::to_writer(bw, &all_info)?;
    }
    return Ok(all_info)

}

fn retrieve_station() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let subway_url = "https://www.mbta.com/stops/subway#subway-tab";
    let communter_url = "https://www.mbta.com/stops/commuter-rail#commuter-rail-tab";
    let ferry_url = "https://www.mbta.com/stops/ferry#ferry-tab";
    let stations_info = parse_stations(subway_url)?;
    let mut station_conversion: HashMap<String, String> = stations_info.iter().cloned().collect();
    station_conversion.extend(parse_stations(communter_url)?);
    station_conversion.extend(parse_stations(ferry_url)?);
    return Ok(station_conversion)
}

fn parse_stations(url: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let website_text = reqwest::blocking::get(url)?.text()?;
    let document = Html::parse_document(&website_text);
    let selector = Selector::parse(r#"a[class="btn button stop-btn m-detailed-stop"]"#).unwrap();
    let station_select = document.select(&selector);
    let station_conversion: Vec<(String, String)> = station_select
        .map(|button| (
                button
                .value()
                .attr("data-name")
                .unwrap()
                .replace(" ", "_")
                .replace("'", ""), 
                button
                .value()
                .attr("href")
                .unwrap()
                .replace("/stops/", "")
                )
            )
        .collect();
    return Ok(station_conversion)
}

fn retrieve_commuter() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let commuter_url = "https://www.mbta.com/schedules/commuter-rail";
    let commuter_info = parse_commuter(commuter_url)?;
    let commuter_conversion: HashMap<String, String> = commuter_info.iter().cloned().collect();
    return Ok(commuter_conversion)
}

fn parse_commuter(url: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
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
