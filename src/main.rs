use crate::parser::parse_netex;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use petgraph::algo::{astar, dijkstra};
use petgraph::visit::EdgeRef;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::SystemTime;

mod parser;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut graph = petgraph::graphmap::DiGraphMap::new();
    let mut vert2idx = HashMap::new();
    let mut idx2vert = HashMap::new();
    let mut same_vert: HashMap<String, BTreeMap<NaiveTime, usize>> = HashMap::new();
    let mut vert_counter = 0;


    let mut counter = 0;
    let start = SystemTime::now();

    let mut connections = Vec::new();

    if Path::new("sample-all/cache.bin").is_file() {
        println!("Loading from cache");
        let file = File::open("sample-all/cache.bin")?;
        connections = bincode::deserialize_from(file)?;
    } else {
        for entry in Path::new("sample-all").read_dir()? {
            if let Ok(entry) = entry {
                if entry.path().is_file() && entry.path().extension() == Some("xml".as_ref()) {
                    if counter % 100 == 0 {
                        println!("parsing {} {}", counter, entry.path().display());
                    }
                    counter += 1;
                    let connection = parse_netex(entry.path())?;
                    connections.push(connection);
                }
            }
        }
        println!("Caching...");
        let mut file = File::create("sample-all/cache.bin")?;
        bincode::serialize_into(&file, &connections)?;
        file.flush()?;
    }

    println!("Creating graph...");

    for connection in connections {
        for stop_name in &connection.stops {
            if !same_vert.contains_key(stop_name) {
                same_vert.insert(stop_name.clone(), BTreeMap::<NaiveTime, usize>::new());
            }
        }
        for journey in &connection.journeys {
            if journey.is_valid(&connection, NaiveDateTime::from(NaiveDate::from_ymd_opt(2024, 11, 4).unwrap())) {
                for i in 0..journey.passings.len() - 1 {
                    let start_st = &journey.passings[i];
                    let end_st = &journey.passings[i+1];
                    // don't go back in time
                    if end_st.arrival <= start_st.departure {
                        continue;
                    }
                    let start_name = connection.stops[start_st.stop_point].clone() + ";" + &start_st.departure.unwrap().to_string().clone();
                    let end_name = connection.stops[end_st.stop_point].clone() + ";" + &end_st.arrival.unwrap().to_string().clone();
                    if !vert2idx.contains_key(&start_name) {
                        vert2idx.insert(start_name.clone(), vert_counter);
                        idx2vert.insert(vert_counter, start_name.clone());
                        same_vert.get_mut(&connection.stops[start_st.stop_point].clone()).unwrap().insert(start_st.departure.unwrap(), vert_counter);
                        vert_counter += 1;
                    }
                    if !vert2idx.contains_key(&end_name) {
                        vert2idx.insert(end_name.clone(), vert_counter);
                        idx2vert.insert(vert_counter, end_name.clone());
                        same_vert.get_mut(&connection.stops[end_st.stop_point].clone()).unwrap().insert(end_st.arrival.unwrap(), vert_counter);
                        vert_counter += 1;
                    }
                    graph.add_edge(
                        vert2idx[&start_name],
                        vert2idx[&end_name],
                        (end_st.arrival.unwrap() - start_st.departure.unwrap()).num_seconds()
                    );
                }
            }
        }
    }

    println!("{} {}", vert_counter, same_vert.keys().len());
    println!("{:?}", start.elapsed().expect("Failed to get elapsed time"));
    for (_, verts) in &same_vert {
        if verts.len() < 2 {
            continue;
        }
        let mut iter = verts.iter();
        let (mut start_t, mut start_vert) = iter.next().unwrap();
        for (end_t, end_vert) in iter {
            graph.add_edge(*start_vert, *end_vert, (end_t.clone() - start_t.clone()).num_seconds());
            start_vert = end_vert;
            start_t = end_t;
        }
    }
    let end_vert: Vec<usize> = same_vert["Liberec"].iter().map(|(k, v)| *v).collect();

    for (_, start_vert) in &same_vert["Opočno,,nám."] {
        println!("start {}", idx2vert[&start_vert]);
        let scores = astar(&graph, *start_vert, |f| end_vert.contains(&f), |e| *e.weight(), |_| 0).unwrap();
        println!("cost: {}", scores.0);
        for vert in scores.1 {
            print!("{} ", idx2vert[&vert]);
        }
        println!();
        println!();
    }
    // let scores = dijkstra(&graph, *start_vert, None, |e| *e.weight());
    // for (vert, score) in scores {
    //     let dt = NaiveTime::from_num_seconds_from_midnight_opt(score as u32, 0);
    //     if let Some(dt) = dt {
    //         println!("{} -> {} {}", score, idx2vert[&vert], dt);
    //     } else {
    //         println!("{} -> {}", score, idx2vert[&vert]);
    //     }
    // }
    Ok(())
}