use bit_set::BitSet;
use chrono::{NaiveDateTime, NaiveTime};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug)]
struct ParsedOperatingPeriod {
    from_date: Option<NaiveDateTime>,
    to_date: Option<NaiveDateTime>,
    day_bits: Option<BitSet>
}

#[derive(Debug)]
struct ParsedServiceJourney {
    valid_from: Option<NaiveDateTime>,
    valid_to: Option<NaiveDateTime>,
    day_types: Vec<String>,
    pattern: Option<String>,
    passings: Vec<ParsedPassing>,
}

#[derive(Debug)]
struct ParsedPassing {
    stop_point: Option<String>,
    departure: Option<NaiveTime>,
    arrival: Option<NaiveTime>,
}

#[derive(Debug)]
struct ParsedJourneyPattern {
    order: BTreeMap<i32, String>,
    points: HashMap<String, String>,
}

macro_rules! netex_frames {
    // taken from vec! macro
    ($($x:expr),+ $(,)?) => (
        <[_]>::into_vec(
            std::boxed::Box::new(["PublicationDelivery", "dataObjects", "CompositeFrame", "frames", $($x),+])
        )
    );
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
pub struct Passing {
    // index of stop in connection stops
    pub stop_point: usize,
    pub arrival: Option<NaiveTime>,
    pub departure: Option<NaiveTime>,
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
    pub fn is_valid(&self, parent: &Connection, date: NaiveDateTime) -> bool {
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
        return false;
    }
}

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

pub fn parse_netex<P: AsRef<Path>>(file_path: P) -> Result<Connection, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_file(file_path)?;

    let mut path = Vec::with_capacity(64);
    let mut buffer = Vec::new();

    let mut id = None;
    let mut id_pattern = None;
    let mut ref_op_period = None;
    let mut ref_day_type = None;

    // ScheduledStopPoint - station name map
    let mut sched_stop2name = HashMap::new();
    // list of DayType
    let mut day_types = Vec::new();

    let mut operating_perdios: HashMap<String, ParsedOperatingPeriod> = HashMap::new();
    let mut day_type2op_period: HashMap<String, String> = HashMap::new();
    let mut journey_patterns: HashMap<String, ParsedJourneyPattern> = HashMap::new();
    let mut service_journeys: Vec<ParsedServiceJourney> = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(ref e)) => {
                path.push(String::from_utf8(Vec::from(e.name().0)).unwrap());
                if path_vec_eq(&path, netex_frames![
                    "ServiceFrame", "scheduledStopPoints", "ScheduledStopPoint"
                ]) {
                    id = Some(e.try_get_attribute("id")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceFrame", "journeyPatterns", "ServiceJourneyPattern"
                ]) {
                    id_pattern = Some(e.try_get_attribute("id")?.unwrap().unescape_value()?.to_string());
                    journey_patterns.insert(id_pattern.clone().unwrap().clone(), ParsedJourneyPattern {
                        order: BTreeMap::new(),
                        points: HashMap::new(),
                    });
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceFrame", "journeyPatterns", "ServiceJourneyPattern", "pointsInSequence", "StopPointInJourneyPattern"
                ]) {
                    id = Some(e.try_get_attribute("id")?.unwrap().unescape_value()?.to_string());
                    let order = i32::from_str(&*e.try_get_attribute("order")?.unwrap().unescape_value()?)?;
                    journey_patterns.get_mut(&id_pattern.clone().unwrap()).unwrap().order.insert(order, id.clone().unwrap().clone());
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "operatingPeriods", "UicOperatingPeriod"
                ]) {
                    id = Some(e.try_get_attribute("id")?.unwrap().unescape_value()?.to_string());
                    operating_perdios.insert(e.try_get_attribute("id")?.unwrap().unescape_value()?.to_string(), ParsedOperatingPeriod {
                        from_date: Default::default(),
                        to_date: Default::default(),
                        day_bits: Default::default(),
                    });
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney"
                ]) {
                    service_journeys.push(ParsedServiceJourney {
                        valid_from: None,
                        valid_to: None,
                        day_types: Vec::new(),
                        pattern: None,
                        passings: Vec::new()
                    });
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "passingTimes", "TimetabledPassingTime"
                ]) {
                    service_journeys.last_mut().unwrap().passings.push(ParsedPassing {
                        stop_point: None,
                        departure: None,
                        arrival: None,
                    })
                }
            }
            Ok(Event::Empty(e)) => {
                path.push(String::from_utf8(Vec::from(e.name().0)).unwrap());
                if path_vec_eq(&path, netex_frames![
                    "ServiceFrame", "journeyPatterns", "ServiceJourneyPattern", "pointsInSequence", "StopPointInJourneyPattern", "ScheduledStopPointRef"
                ]) {
                    journey_patterns.get_mut(&id_pattern.clone().unwrap()).unwrap().points
                        .insert(id.clone().unwrap().clone(), e.try_get_attribute("ref")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "dayTypes", "DayType"
                ]) {
                    day_types.push(e.try_get_attribute("id")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "dayTypeAssignments", "DayTypeAssignment", "OperatingPeriodRef"
                ]) {
                    ref_op_period = Some(e.try_get_attribute("ref")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "dayTypeAssignments", "DayTypeAssignment", "DayTypeRef"
                ]) {
                    ref_day_type = Some(e.try_get_attribute("ref")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "dayTypes", "DayTypeRef"
                ]) {
                    service_journeys.last_mut().unwrap().day_types.push(e.try_get_attribute("ref")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "ServiceJourneyPatternRef"
                ]) {
                    service_journeys.last_mut().unwrap().pattern = Some(e.try_get_attribute("ref")?.unwrap().unescape_value()?.to_string());
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "passingTimes", "TimetabledPassingTime", "StopPointInJourneyPatternRef"
                ]) {
                    service_journeys.last_mut().unwrap().passings.last_mut().unwrap().stop_point = Some(e.try_get_attribute("ref")?.unwrap().unescape_value()?.to_string());
                }
                path.pop();
            }
            Ok(Event::End(_)) => {
                if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "dayTypeAssignments", "DayTypeAssignment"
                ]) {
                    day_type2op_period.insert(ref_day_type.unwrap(), ref_op_period.unwrap());
                    ref_day_type = None;
                    ref_op_period = None;
                }
                path.pop();
            }
            Ok(Event::Text(e)) => {
                if path_vec_eq(&path, netex_frames![
                    "ServiceFrame", "scheduledStopPoints", "ScheduledStopPoint", "Name"
                ]) {
                    let a = e.unescape()?.to_string();
                    sched_stop2name.insert(id.clone().unwrap(), a);
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "operatingPeriods", "UicOperatingPeriod", "FromDate"
                ]) {
                    operating_perdios.get_mut(&id.clone().unwrap()).unwrap().from_date = Some(NaiveDateTime::parse_from_str(&e.unescape()?, "%Y-%m-%dT%H:%M:%S")?);
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "operatingPeriods", "UicOperatingPeriod", "ToDate"
                ]) {
                    operating_perdios.get_mut(&id.clone().unwrap()).unwrap().to_date = Some(NaiveDateTime::parse_from_str(&e.unescape()?, "%Y-%m-%dT%H:%M:%S")?);
                } else if path_vec_eq(&path, netex_frames![
                    "ServiceCalendarFrame", "ServiceCalendar", "operatingPeriods", "UicOperatingPeriod", "ValidDayBits"
                ]) {
                    let a = e.unescape()?.to_string();
                    let bool_vec: Vec<bool> = a.chars().map(|c| c == '1').collect();
                    let mut bits = BitSet::new();
                    for i in 0..bool_vec.len() {
                        if bool_vec[i] {
                            bits.insert(i);
                        }
                    }
                    operating_perdios.get_mut(&id.clone().unwrap()).unwrap().day_bits = Some(bits);
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "ValidBetween", "FromDate"
                ]) {
                    service_journeys.last_mut().unwrap().valid_from = Some(NaiveDateTime::parse_from_str(&e.unescape()?, "%Y-%m-%dT%H:%M:%S")?);
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "ValidBetween", "ToDate"
                ]) {
                    service_journeys.last_mut().unwrap().valid_to = Some(NaiveDateTime::parse_from_str(&e.unescape()?, "%Y-%m-%dT%H:%M:%S")?);
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "passingTimes", "TimetabledPassingTime", "DepartureTime"
                ]) {
                    service_journeys.last_mut().unwrap().passings.last_mut().unwrap().departure = Some(NaiveTime::parse_from_str(&e.unescape()?, "%H:%M:%S")?);
                } else if path_vec_eq(&path, netex_frames![
                    "TimetableFrame", "vehicleJourneys", "ServiceJourney", "passingTimes", "TimetabledPassingTime", "ArrivalTime"
                ]) {
                    service_journeys.last_mut().unwrap().passings.last_mut().unwrap().arrival = Some(NaiveTime::parse_from_str(&e.unescape()?, "%H:%M:%S")?);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            Ok(_) => { }
        }
    }

    let mut new_op_periods = Vec::new();
    let mut idx_op_periods = HashMap::new();
    for (name, data) in operating_perdios {
        idx_op_periods.insert(name, new_op_periods.len());
        new_op_periods.push(OperatingPeriod {
            from_date: data.from_date.unwrap(),
            to_date: data.to_date.unwrap(),
            day_bits: data.day_bits.unwrap(),
        });
    }

    let mut new_day_types = Vec::new();
    let mut idx_day_types = HashMap::new();
    for day_type in day_types {
        idx_day_types.insert(day_type.clone(), new_day_types.len());
        if let Some(period) = day_type2op_period.get(&day_type) {
            new_day_types.push(Some(idx_op_periods[period]));
        } else {
            new_day_types.push(None);
        }
    }

    let mut new_stops = Vec::new();
    let mut idx_stops = HashMap::new();
    for (stop, name) in sched_stop2name {
        idx_stops.insert(stop, new_stops.len());
        new_stops.push(name);
    }

    let mut new_patterns = Vec::new();
    let mut idx_patterns = HashMap::new();
    for (name, pattern) in journey_patterns {
        idx_patterns.insert(name, new_patterns.len());
        let mut sub_pattern = Vec::new();
        for (_, stop_point) in pattern.order {
            sub_pattern.push((stop_point.clone(), idx_stops[&pattern.points[&stop_point]]));
        }
        new_patterns.push(sub_pattern);
    }

    let mut new_journeys = Vec::new();
    for parsed_journey in service_journeys {
        let pattern_idx = idx_patterns[&parsed_journey.pattern.unwrap()];
        let mut days = Vec::new();
        for day_type in parsed_journey.day_types {
            days.push(idx_day_types[&day_type]);
        }
        let valid_from = parsed_journey.valid_from.unwrap();
        let valid_to = parsed_journey.valid_to.unwrap();
        let mut passings = HashMap::new();
        for parsed_passing in parsed_journey.passings {
            passings.insert(parsed_passing.stop_point.unwrap(), (parsed_passing.arrival, parsed_passing.departure));
        }
        let mut new_passings = Vec::new();
        for (sched_point, stop) in &new_patterns[pattern_idx] {
            new_passings.push(Passing {
                stop_point: *stop,
                arrival: passings[sched_point].0,
                departure: passings[sched_point].1,
            });
        }
        new_journeys.push(Journey {
            passings: new_passings,
            valid_from,
            valid_to,
            days,
        })
    }

    Ok(Connection{
        operating_periods: new_op_periods,
        day_types: new_day_types,
        stops: new_stops,
        journeys: new_journeys,
    })
}

fn path_vec_eq(left_path: &Vec<String>, rigth_path: Vec<&str>) -> bool {
    if left_path.len() != rigth_path.len() {
        return false;
    }
    left_path.iter().zip(rigth_path.iter()).all(|(a, b)| a == b)
}