use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use petgraph::prelude::{Directed, GraphMap};
use std::collections::HashMap;
use crate::structure::MultiConnection;


#[derive(Eq, Hash, PartialEq, Clone, Debug)]
pub enum Vertex {
    PlaceTime(String, NaiveTime),
    Initial(String),
    Final(String)
}


pub struct ConnectionGraph {
    pub graph: GraphMap<usize, i64, Directed>,
    // vert2idx: HashMap<String, usize>,
    pub idx2vert: HashMap<usize, Vertex>,
    pub vert2idx: HashMap<Vertex, usize>,
    // pub same_vert: HashMap<String, BTreeMap<NaiveTime, usize>>,
    count: usize,
}

impl ConnectionGraph {
    fn get_or_insert(&mut self, vert: &Vertex) -> usize
    {
        if let Some(existing) = self.vert2idx.get(vert) {
            return *existing;
        }

        let id = self.count;
        self.vert2idx.insert(vert.clone(), id);
        self.idx2vert.insert(id, vert.clone());
        self.count += 1;
        if let Vertex::PlaceTime(place, _) = vert {
            let init = self.get_or_insert(&Vertex::Initial(place.clone()));
            let fin = self.get_or_insert(&Vertex::Final(place.clone()));

            self.graph.add_edge(init, id, 0);
            self.graph.add_edge(id, fin, 0);
        }

        return id;
    }

    fn build_waiting_edges(&mut self, station_names: &Vec<String>)
    {
        // behold: the most roundabout shooting into one self's foot
        for station in station_names {
            if let Some(init_id) = self.vert2idx.get(&Vertex::Initial(station.clone())) {
                let mut station_times: Vec<&Vertex>
                    = self.graph.neighbors_directed(
                        *init_id,
                        petgraph::Direction::Outgoing
                    ).map(|id| {
                        self.idx2vert.get(&id).expect(
                            "All neigbours of an initial node should be addded via `get_or_insert`"
                        )
                    }).collect();
                
                station_times.sort_unstable_by(|a, b| {
                    if let &&Vertex::PlaceTime(_, time_a) = a {
                    if let &&Vertex::PlaceTime(_, time_b) = b {
                        return time_a.cmp(&time_b);
                    }}
                    panic!("All folowers of inital vert should be place-time nodes");
                });

                let mut iter = station_times.iter();
                if let Some(last_vert) = iter.next() {
                    let mut last_id = self.vert2idx.get(last_vert).expect(
                        "vert->id should be assigned in `get_or_insert`"
                    );
                    let mut last_time: NaiveTime;
                    if let Vertex::PlaceTime(_, last_time_) = last_vert {
                        last_time = *last_time_;
                    }
                    else { panic!("All folowers of inital vert should be place-time nodes"); }

                    for curr_vert in iter {
                        let curr_id = self.vert2idx.get(curr_vert).expect(
                            "vert->id should be assigned in `get_or_insert`"
                        );
                        let curr_time;
                        if let Vertex::PlaceTime(_, curr_time_) = curr_vert {
                            curr_time = *curr_time_;
                        }
                        else { panic!("All folowers of inital vert should be place-time nodes"); }

                        self.graph.add_edge(*last_id, *curr_id, (curr_time - last_time).num_seconds());
                        last_id = curr_id;
                        last_time = curr_time;
                    }
                }
            }
        }
    }

    pub fn new(connections: &MultiConnection)
    -> ConnectionGraph
    {
        println!("Creating graph...");
        let mut res = ConnectionGraph {
            graph: petgraph::graphmap::DiGraphMap::new(),
            vert2idx: HashMap::new(),
            idx2vert: HashMap::new(),
            count: 0,
        };

        for connection in &connections.connections {
            for journey in &connection.journeys {
                if journey.is_valid(&connection, NaiveDateTime::from(NaiveDate::from_ymd_opt(2024, 11, 4).unwrap())) {
                    for i in 0..journey.passings.len() - 1 {
                        let start_st = &journey.passings[i];
                        let end_st = &journey.passings[i + 1];
                        // don't go back in time
                        if end_st.arrival <= start_st.departure {
                            continue;
                        }

                        let start_id = res.get_or_insert(
                            &Vertex::PlaceTime(
                                connections.stops[start_st.stop_point].clone(),
                                start_st.departure.unwrap().clone(),
                            )
                        );

                        let end_id = res.get_or_insert(
                            &Vertex::PlaceTime(
                                connections.stops[end_st.stop_point].clone(),
                                end_st.arrival.unwrap().clone(),
                            )
                        );

                        res.graph.add_edge(
                            start_id,
                            end_id,
                            (end_st.arrival.unwrap() - start_st.departure.unwrap()).num_seconds()
                        );
                    }
                }
            }
        }

        res.build_waiting_edges(&connections.stops);

        return res;
    }
}