use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use chrono::NaiveDateTime;
use bit_set::BitSet;
use std::fmt::Display;
use crate::parser::Passing;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum StopPlaceType {
    RailStation,
    Other,
    Unknown
}

impl StopPlaceType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "railStation" => StopPlaceType::RailStation,
            "other" => StopPlaceType::Other,
            _ => {
                println!("Unknown stop place type: {}", s);
                StopPlaceType::Unknown
            }
        }
    }
}

impl Display for StopPlaceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            StopPlaceType::RailStation => { String::from("RailStation") }
            StopPlaceType::Other => { String::from("Other") }
            StopPlaceType::Unknown => { String::from("Unknown") }
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OperatingPeriod {
    pub from_date: NaiveDateTime,
    pub to_date: NaiveDateTime,
    pub day_bits: BitSet
}

impl OperatingPeriod {
    pub fn is_valid(&self, date: NaiveDateTime) -> bool {
        if self.from_date > date || date > self.to_date {
            return false;
        }
        let delta = date - self.from_date;
        self.day_bits.contains(delta.num_days() as usize)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Journey {
    // sequence of passings
    pub passings: Vec<Passing>,
    pub valid_from: NaiveDateTime,
    pub valid_to: NaiveDateTime,
    // index of day type
    pub days: Vec<usize>,
}

impl Journey {
    pub fn is_valid(&self, parent: &SubMultiConnection, date: NaiveDateTime) -> bool {
        if self.valid_from > date || date > self.valid_to {
            return false;
        }
        for day_idx in &self.days {
            if let Some(period_idx) = parent.day_types[*day_idx] {
                if parent.operating_periods[period_idx].is_valid(date) {
                    return true;
                }
            }
        }
        false
    }
}

// TODO: add option to merge stops from multiple connections
#[derive(Debug, Serialize, Deserialize)]
pub struct Connection {
    pub operating_periods: Vec<OperatingPeriod>,
    // index of operating period in operating periods
    pub day_types: Vec<Option<usize>>,
    // stop names by index
    pub stops: Vec<String>,
    pub journeys: Vec<Journey>
}

impl Connection {
    pub fn print_journey(&self, index: usize) {
        if self.journeys.len() < index {
            println!("journey {} is out of bounds", index);
            return;
        }
        let journey = &self.journeys[index];
        println!("journey {} with index", index);
        println!("valid from {} to {}", journey.valid_from, journey.valid_to);
        for passing in &journey.passings {
            println!("\t- {:?} - {:?}: {}",
                     passing.arrival.map_or_else(|| String::from(""), |t| t.format("%H:%M:%S").to_string()),
                     passing.departure.map_or_else(|| String::from(""), |t| t.format("%H:%M:%S").to_string()),
                     self.stops[passing.stop_point]);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubMultiConnection {
    pub operating_periods: Vec<OperatingPeriod>,
    // index of operating period in operating periods
    pub day_types: Vec<Option<usize>>,
    pub journeys: Vec<Journey>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MultiConnection {
    // stop names by index
    pub stops: Vec<String>,
    pub connections: Vec<SubMultiConnection>,
}

impl From<Vec<Connection>> for MultiConnection {
    fn from(value: Vec<Connection>) -> Self {
        let mut stop_counter = 0;
        let mut new_stops = Vec::new();
        let mut idx_stop = HashMap::new();
        let mut sub_conns = Vec::new();
        for connection in value {
            let mut idx_sub_stop = HashMap::new();
            let mut sub_stop_counter = 0;
            for stop in connection.stops {
                if !idx_stop.contains_key(&stop) {
                    idx_stop.insert(stop.clone(), stop_counter);
                    new_stops.push(stop.clone());
                    stop_counter += 1;
                }
                idx_sub_stop.insert(sub_stop_counter, idx_stop[&stop]);
                sub_stop_counter += 1;
            }
            let mut new_journeys = Vec::new();
            for journey in connection.journeys {
                new_journeys.push(Journey {
                    passings: journey.passings.iter().map(|p| Passing {
                        stop_point: idx_sub_stop[&p.stop_point],
                        arrival: p.arrival,
                        departure: p.departure,
                    }).collect(),
                    valid_from: journey.valid_from,
                    valid_to: journey.valid_to,
                    days: journey.days,
                });
            }
            sub_conns.push(SubMultiConnection {
                operating_periods: connection.operating_periods,
                day_types: connection.day_types,
                journeys: new_journeys,
            })
        }
        MultiConnection {
            stops: new_stops,
            connections: sub_conns,
        }
    }
}