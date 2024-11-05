use crate::parser::parse_netex;
use chrono::NaiveTime;
use connection_graph::Vertex;
use petgraph::algo::{astar, dijkstra};
use petgraph::visit::EdgeRef;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;
use clap::Parser;
use flate2::Compression;
use flate2::read::ZlibDecoder;
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
    let mut file = ZlibEncoder::new(File::create(&cache_path)?, Compression::default());
    bincode::serialize_into(&mut file, &connections)?;
    file.flush()?;
    Ok(())
}

fn load_netex(path: &PathBuf, invalidate_cache: bool)
-> Result<MultiConnection, Box<dyn std::error::Error>>
{
    let connections: MultiConnection;
    
    let data_cache = path.join("cache.bin");

    if (!invalidate_cache) && data_cache.is_file() {
        println!("Loading from cache");
        let file = ZlibDecoder::new(File::open(data_cache)?);
        connections = bincode::deserialize_from(file)?;
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
    let g = ConnectionGraph::new(&connections);

    println!("{:?}", start.elapsed().expect("Failed to get elapsed time"));
    let start = g.vert2idx.get(&Vertex::Initial("Opočno,,nám./Other".to_string()));
    let end = g.vert2idx.get(&Vertex::Final("Hradec Králové,,Terminál HD/Other".to_string()));

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
        *start.unwrap(),
        Some(*end.unwrap()),
        // None,
        |e| *e.weight()
    );
    for (vert, score) in scores {
        if let Vertex::Final(stop) = &g.idx2vert[&vert] {
            let dt = NaiveTime::from_num_seconds_from_midnight_opt(score as u32, 0);
            if let Some(dt) = dt {
                println!("{} -> {} {}", score, stop, dt);
            } else {
                println!("{} -> {}", score, stop);
            }
        }
    }
    Ok(())
}