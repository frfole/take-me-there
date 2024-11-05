use chrono::{NaiveDateTime, NaiveTime};
use petgraph::prelude::{Directed, GraphMap};
use std::collections::{BTreeMap, HashMap};
use crate::structure::MultiConnection;


#[derive(Debug)]
pub struct Station<V> {
    pub init: V,
    pub fin: V,
    pub times: BTreeMap<NaiveTime, V>,
}


pub struct ConnectionGraph {
    pub graph: GraphMap<usize, i64, Directed>,
    pub stations: HashMap<String, Station<usize>>,
    stations_by_ids: BTreeMap<usize, String>,
}

#[derive(Debug)]
pub enum StopKind {
    Initial,
    Terminal,
    At(NaiveTime),
}

#[derive(Debug)]
pub struct Stop {
    name: String,
    detail: StopKind,
}


fn prepare_station_info(names: &Vec<String>)
-> HashMap<String, Station<usize>>
{
    let mut stations: HashMap<String, Station<usize>> = HashMap::with_capacity(names.len());
    for station_name in names {
        if let Some(_) = stations.insert(station_name.clone(), Station {
            fin: 0,
            init: 0,
            times: BTreeMap::new()
        }) {
            eprintln!("Duplicate station name: '{}'", station_name);
        }
    }

    return stations;
}

fn populate_station_info_and_count_verts(
    stations: &mut HashMap<String, Station<usize>>,
    names_by_ids: &mut BTreeMap<usize, String>,
    connections: &MultiConnection,
    date: &NaiveDateTime,
) -> usize {
    for journey in connections.iter_valid_journeys(date)
    {
        for pass in &journey.passings {
            let name = &connections.stops[pass.stop_point];
            if let Some(station) = stations.get_mut(name) {
                if let Some(arr_time) = pass.arrival {
                    station.times.insert(arr_time, 0);
                }

                if let Some(dep_time) = pass.departure {
                    station.times.insert(dep_time, 0);
                }
            }
        }
    }

    let mut vert_count = 0;
    for (name, station) in stations {
        names_by_ids.insert(vert_count, name.clone());
        station.init = vert_count;
        station.fin = vert_count + 1;
        vert_count += 2;

        for (_, vert) in &mut station.times {
            *vert = vert_count;
            vert_count += 1;
        }
    }

    return vert_count;
}

fn connect_between_stations(
    graph: &mut GraphMap<usize, i64, Directed>,
    stations: &HashMap<String, Station<usize>>,
    connections: &MultiConnection,
    date: &NaiveDateTime,
) {
    for journey in connections.iter_valid_journeys(date) {
        for i in 0..journey.passings.len() - 1 {
            let start_st = &journey.passings[i];
            let end_st = &journey.passings[i + 1];

            // don't go back in time
            if end_st.arrival <= start_st.departure {
                continue;
            }

            let start_name = &connections.stops[start_st.stop_point];
            let start_station = &stations[start_name];
            let start_id = &start_station.times[&start_st.departure.unwrap()];

            let end_name = &connections.stops[end_st.stop_point];
            let end_station = &stations[end_name];
            let end_id = &end_station.times[&end_st.arrival.unwrap()];

            graph.add_edge(
                *start_id,
                *end_id,
                (end_st.arrival.unwrap() - start_st.departure.unwrap()).num_seconds()
            );
        }
    }
}

fn connect_within_stations(
    graph: &mut GraphMap<usize, i64, Directed>,
    stations: &HashMap<String, Station<usize>>,
) {
    // IDEA: joinig last to first would enable night transfers
    for (_, station) in stations {
        let mut iter = station.times.iter();
        if let Some(mut last) = iter.next() {
            graph.add_edge(*last.1, *&station.fin, 0);
            graph.add_edge(*&station.init, *last.1, 0);

            for vert in iter {
                graph.add_edge(*last.1, *vert.1, (*vert.0 - *last.0).num_seconds());
                graph.add_edge(*vert.1, *&station.fin, 0);
                graph.add_edge(*&station.init, *vert.1, 0);
                last = vert;
            }
        }
    }
}

impl ConnectionGraph {
    pub fn terminal_by_id(&self, id: &usize) -> Option<String> {
        use std::ops::Bound::*;
        if let Some((_, name)) = self.stations_by_ids.range(( Unbounded, Included(id) )).next_back() {
            let station = &self.stations[name];
            if &station.fin == id {
                return Some(name.clone());
            }
        }

        return None;
    }

    pub fn stop_by_id(&self, id: &usize) -> Option<Stop>
    {
        use std::ops::Bound::*;
        if let Some((_, name)) = self.stations_by_ids.range(( Included(id), Unbounded )).next() {
            let station = &self.stations[name];
            use StopKind::*;
            if &station.init == id {
                return Some(Stop { name: name.clone(), detail: Initial, });
            } else if &station.fin == id {
                return Some(Stop { name: name.clone(), detail: Terminal, });
            }

            for (time, vert) in &station.times {
                if vert == id {
                    return Some(Stop { name: name.clone(), detail: At(time.clone()), });
                }
            }
        }

        return None;
    }

    pub fn new(connections: &MultiConnection, date: &NaiveDateTime)
    -> ConnectionGraph
    {
        eprintln!("Creating graph...");

        eprintln!("Building stations");
        let mut stations = prepare_station_info(&connections.stops);
        let mut names_by_ids = BTreeMap::new();
        let vert_count = populate_station_info_and_count_verts(
            &mut stations,
            &mut names_by_ids,
            connections,
            date
        );

        eprintln!("{vert_count} verts");

        eprintln!("Allocating");
        let mut graph
        = petgraph::graphmap::DiGraphMap::with_capacity(vert_count, 3 * vert_count);

        eprintln!("Connecting stations");
        connect_between_stations(&mut graph, &stations, connections, date);

        eprintln!("Adding in-station edges");
        connect_within_stations(&mut graph, &stations);

        eprintln!("Graph complete");
        return ConnectionGraph {
            graph,
            stations,
            stations_by_ids: names_by_ids,
        };
    }
}