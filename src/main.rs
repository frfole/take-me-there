use crate::parser::parse_netex;
use chrono::NaiveTime;
use petgraph::algo::{astar, dijkstra};
use petgraph::visit::EdgeRef;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::time::SystemTime;
use clap::Parser;
use flate2::Compression;
use flate2::bufread::ZlibDecoder;
use flate2::write::ZlibEncoder;
use crate::structure::MultiConnection;
use crate::connection_graph::ConnectionGraph;

mod parser;
mod structure;
mod connection_graph;

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
        eprintln!("Loading from cache");
        let reader = ZlibDecoder::new(BufReader::new(File::open(data_cache)?));
        connections = bincode::deserialize_from(reader)?;
    } else {
        let mut counter = 0;
        let mut sub_conns = Vec::new();
        for entry in path.read_dir()? {
            if let Ok(entry) = entry {
                if entry.path().is_file() && entry.path().extension() == Some("xml".as_ref()) {
                    if counter % 100 == 0 {
                        eprintln!("parsing {} {}", counter, entry.path().display());
                    }
                    counter += 1;
                    let connection = parse_netex(entry.path())?;
                    sub_conns.push(connection);
                }
            }
        }
        connections = MultiConnection::from(sub_conns);
        eprintln!("Caching...");
        if let Err(e) = save_netex_cache(&data_cache, &connections) {
            eprintln!("Failed to save cache:\n {}", e);
        }
    }

    Ok(connections)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut start = SystemTime::now();

    let connections = load_netex(&args.data_path, args.invalidate_cache)?;

    eprintln!("Connections loaded in {:?}", start.elapsed().expect("Failed to get elapsed time"));
    start = SystemTime::now();

    let g = ConnectionGraph::new(&connections);

    eprintln!("Graph built in {:?}", start.elapsed().expect("Failed to get elapsed time"));

    let start_station = &g.stations["Benešov,Vidlákova Lhota,rozc./Other"].init;
    // let end_station = &g.stations["Hradec Králové,,Terminál HD/Other"].fin;

    // eprintln!("{:?} -> {:?}", g.stop_by_id(start_station), g.terminal_by_id(end_station));

    /*
    let end_vert: Vec<usize> = g.same_vert["Hradec Králové,,Terminál HD/Other"].iter().map(|(_, v)| *v).collect();

    for (_, start_vert) in &g.same_vert["Opočno,,nám./Other"] {
        println!("start {}", g.idx2vert[&start_vert]);
        let score = astar(&g.graph, *start_vert, |f| end_vert.contains(&f), |e| *e.weight(), |_| 0);
        if let Some((cost, path)) = score {
            println!("cost: {}", cost);
            for vert in path {
                print!("{} ", g.idx2vert[&vert]);
            }
            println!();
            println!();
        }
    }
    */
    let scores = dijkstra(
        &g.graph,
        *start_station,
        // Some(*end_station),
        None,
        |e| *e.weight()
    );
    for (vert, score) in &scores {
        // if let Vertex::Final(stop) = &g.idx2vert[&vert] {
        if let Some(name) = &g.terminal_by_id(vert) {
            let dt = NaiveTime::from_num_seconds_from_midnight_opt(*score as u32, 0);
            if let Some(dt) = dt {
                println!("{score} -> {name} {dt}");
            } else {
                println!("{score} -> {name}");
            }
        }
    }
    Ok(())
}