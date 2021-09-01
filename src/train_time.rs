use reqwest;
use std;
use chrono::prelude::*;
use chrono::{DateTime, Local, TimeZone};
use serde_json::Value;
use std::{collections::HashMap, error::Error};

// Main function to retrieve train times from Forest Hills Station for inbound commuter rail
pub async fn train_times(
    dir_code: &str,
    station: &str,
    route_code: &str,
) -> Result<Option<Vec<DateTime<Local>>>, Box<dyn Error>> {
    // get prediction times
    let prediction_times_task = get_prediction_times(station, dir_code, route_code);
    // get schuduled times, if None, create empty hashmap
    let scheduled_times_task =
        get_scheduled_times(station, dir_code, route_code, true);
    let prediction_times = prediction_times_task.await?;
    let mut scheduled_times = scheduled_times_task.await?.unwrap_or(HashMap::new());
    // let (prediction_times, scheduled_times_start) = try_join!(prediction_times_task, scheduled_times_task)?;
    // let mut scheduled_times = scheduled_times_start.unwrap_or(HashMap::new());
    // if there are predicted times, replace the scheduled times with the more accurate predicted
    // tiem
    if let Some(pred_times) = prediction_times {
        for key in pred_times.keys() {
            if scheduled_times.keys().any(|schud_key| schud_key == key) {
                *scheduled_times.get_mut(key).unwrap() = pred_times[key]
            } else {
                scheduled_times.insert(key.clone(), pred_times[key]);
            }
        }
    }
    // get the current time and filter out any train time before now
    let now = Local::now();
    let mut all_times = scheduled_times
        .values()
        .filter_map(|date| {
            if date > &now {
                Some(date.clone())
            } else {
                None
            }
        })
        .collect::<Vec<DateTime<Local>>>();
    all_times.sort();
    //    println!("{:?}", all_times);
    if all_times.len() == 0usize {
        return Ok(None);
    }
    return Ok(Some(all_times));
}

pub async fn max_min_times(
    dir_code: &str,
    station: &str,
    route_code: &str,
) -> Result<Option<[DateTime<Local>; 2]>, Box<dyn Error>> {
    if let Some(scheduled_times) = get_scheduled_times(station, dir_code, route_code, false).await? {
        let mut all_times = scheduled_times
            .values()
            .map(|date| date.clone())
            .collect::<Vec<DateTime<Local>>>();
        all_times.sort();
        if let Some(last_vehicle) = all_times.last() {
            return Ok(Some([*last_vehicle, all_times[0]]));
        } else {
            return Ok(None);
        }
    } else {
        return Ok(None);
    };
}

/// Retreived MBTA predicted times with their API
async fn get_prediction_times(
    station: &str,
    dir_code: &str,
    route_code: &str,
) -> Result<Option<HashMap<String, DateTime<Local>>>, Box<dyn Error>> {
    // MBTA API for predicted times
    let address = format!("https://api-v3.mbta.com/predictions?filter[stop]={}&filter[direction_id]={}&include=stop&filter[route]={}", station, dir_code, route_code);
    return get_route_times(address).await;
}

/// Retreived MBTA scheduled times with their API
async fn get_scheduled_times(
    station: &str,
    dir_code: &str,
    route_code: &str,
    filter_time: bool,
) -> Result<Option<HashMap<String, DateTime<Local>>>, Box<dyn std::error::Error>> {
        let address;
    if filter_time {
        let now = chrono::Local::now();
        // MBTA API for scheduled times
        address = format!("https://api-v3.mbta.com/schedules?include=route,trip,stop&filter[min_time]={}%3A{}&filter[stop]={}&filter[route]={}&filter[direction_id]={}",now.hour(), now.minute(), station, route_code, dir_code);
    } else {
        address = format!("https://api-v3.mbta.com/schedules?include=route,trip,stop&filter[stop]={}&filter[route]={}&filter[direction_id]={}", station, route_code, dir_code);
    }
    return get_route_times(address).await;
}

/// Retreives the JSON from MBTA API and parses it into a hasmap
async fn get_route_times(
    address: String,
) -> Result<Option<HashMap<String, DateTime<Local>>>, Box<dyn Error>> {
    // retrieve the routes with the MBTA API returning a converted JSON format
    let routes_json: Value = reqwest::get(&address).await?.json().await?;
    // only interested in the "data" field
    let data_option = routes_json.get("data");
    // if there is a "data" field, proceed
    if let Some(data) = data_option {
        // if the "data" field is an array, proceed
        if let Some(data_array) = data.as_array() {
            // create a new HashMap to put int trip_id and departure time
            let mut commuter_rail_dep_time: HashMap<String, DateTime<Local>> =
                HashMap::new();
            // for each train in the data array, insert the trip_id and departure time
            for train in data_array {
                let departure_time_option = train["attributes"]["departure_time"].as_str();
                let trip_id_option = train["relationships"]["trip"]["data"]["id"].as_str();
                // if there is a trip id
                if let Some(trip_id) = trip_id_option {
                    // and if there is a departure time for the train
                    if let Some(departure_time) = departure_time_option {
                        // convert departure time to DateTime<Local>
                        let departure_time_datetime =
                            Local.datetime_from_str(departure_time, "%+")?;
                        // insert into HashMap
                        commuter_rail_dep_time.insert(trip_id.to_string(), departure_time_datetime);
                    }
                }
            }
            // if successful return the trip_id, departure time HashMap, else return None
            return Ok(Some(commuter_rail_dep_time));
        } else {
            return Ok(None);
        }
    } else {
        return Ok(None);
    };
}
