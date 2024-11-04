use crate::parser::parse_netex;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use petgraph::algo::{astar, dijkstra};
use petgraph::visit::EdgeRef;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::time::SystemTime;
use clap::Parser;
use flate2::Compression;
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use crate::structure::MultiConnection;

mod parser;
mod structure;

#[derive(clap::Parser, Debug)]
#[command(
    version = "0.1",
    about = "A program which calculates the shortest time to get from a \
    specified station to all other stations via public transport.",
    long_about = None
)]
struct Args {
    /// Path to timetables
    #[arg(index = 1)]
    data_path: PathBuf,

    /// Parse time tables even if a parsing cache exists
    #[arg(long, short)]
    invalidate_cache: bool,
}

fn save_netex_cache(cache_path: &PathBuf, connections: &MultiConnection)
-> Result<(), Box<dyn std::error::Error>>
{
    let mut reader = ZlibEncoder::new(BufWriter::new(File::create(&cache_path)?), Compression::default());
    bincode::serialize_into(&mut reader, &connections)?;
    reader.flush()?;
    Ok(())
}

fn load_netex(path: &PathBuf, invalidate_cache: bool)
-> Result<MultiConnection, Box<dyn std::error::Error>>
{
    let connections: MultiConnection;
    
    let data_cache = path.join("cache.bin");

    if (!invalidate_cache) && data_cache.is_file() {
        println!("Loading from cache");
        let reader = ZlibDecoder::new(BufReader::new(File::open(data_cache)?));
        connections = bincode::deserialize_from(reader)?;
    } else {
        let mut counter = 0;
        let mut sub_conns = Vec::new();
        for entry in path.read_dir()? {
            if let Ok(entry) = entry {
                if entry.path().is_file() && entry.path().extension() == Some("xml".as_ref()) {
                    if counter % 100 == 0 {
                        println!("parsing {} {}", counter, entry.path().display());
                    }
                    counter += 1;
                    let connection = parse_netex(entry.path())?;
                    sub_conns.push(connection);
                }
            }
        }
        connections = MultiConnection::from(sub_conns);
        println!("Caching...");
        if let Err(e) = save_netex_cache(&data_cache, &connections) {
            println!("Failed to save cache:\n {}", e);
        }
    }

    Ok(connections)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let start = SystemTime::now();

    let connections = load_netex(&args.data_path, args.invalidate_cache)?;

    println!("Creating graph...");
    let mut graph = petgraph::graphmap::DiGraphMap::new();
    let mut vert2idx = HashMap::new();
    let mut idx2vert = HashMap::new();
    let mut same_vert: HashMap<String, BTreeMap<NaiveTime, usize>> = HashMap::new();
    let mut vert_counter = 0;

    for stop_name in &connections.stops {
        if !same_vert.contains_key(stop_name) {
            same_vert.insert(stop_name.clone(), BTreeMap::<NaiveTime, usize>::new());
        }
    }
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
                    let start_name = connections.stops[start_st.stop_point].clone() + ";" + &start_st.departure.unwrap().to_string().clone();
                    let end_name = connections.stops[end_st.stop_point].clone() + ";" + &end_st.arrival.unwrap().to_string().clone();
                    if !vert2idx.contains_key(&start_name) {
                        vert2idx.insert(start_name.clone(), vert_counter);
                        idx2vert.insert(vert_counter, start_name.clone());
                        same_vert.get_mut(&connections.stops[start_st.stop_point].clone()).unwrap().insert(start_st.departure.unwrap(), vert_counter);
                        vert_counter += 1;
                    }
                    if !vert2idx.contains_key(&end_name) {
                        vert2idx.insert(end_name.clone(), vert_counter);
                        idx2vert.insert(vert_counter, end_name.clone());
                        same_vert.get_mut(&connections.stops[end_st.stop_point].clone()).unwrap().insert(end_st.arrival.unwrap(), vert_counter);
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
    let end_vert: Vec<usize> = same_vert["Hradec Králové,,Terminál HD/Other"].iter().map(|(_, v)| *v).collect();

    for (_, start_vert) in &same_vert["Opočno,,nám./Other"] {
        println!("start {}", idx2vert[&start_vert]);
        let score = astar(&graph, *start_vert, |f| end_vert.contains(&f), |e| *e.weight(), |_| 0);
        if let Some((cost, path)) = score {
            println!("cost: {}", cost);
            for vert in path {
                print!("{} ", idx2vert[&vert]);
            }
            println!();
            println!();
        }
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