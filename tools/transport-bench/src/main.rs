//! Release-mode comparison using the production protobuf decoder, without GUI dependencies.
use shrine_egui::{protobuf,model::{Presentation,Source,Snapshot}};
use serde::Deserialize;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    path::Path,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};

struct Counted;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
fn record(bytes: usize) {
    if TRACK.load(Ordering::Relaxed) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOC: Counted = Counted;
#[derive(Deserialize)]
struct Receipt { #[serde(rename="operationId")] _id:String, #[serde(rename="outcome")] _outcome:String }
#[allow(dead_code)]
enum Decoded { Presentation(Presentation), Source(Source), Snapshot(Snapshot), Receipt(Receipt) }
fn typed<T:serde::de::DeserializeOwned>(binary:bool,bytes:&[u8])->T {
    if binary {protobuf::deserialize(black_box(bytes)).unwrap()} else {serde_json::from_slice(black_box(bytes)).unwrap()}
}
fn decode(name:&str,binary:bool,bytes:&[u8])->Decoded {
    match name {
        "rich-480"|"rich-960"|"deck"=>Decoded::Presentation(typed(binary,bytes)),
        "source"|"tree"=>Decoded::Source(typed(binary,bytes)),
        "status"=>Decoded::Snapshot(typed(binary,bytes)),
        "receipt"=>Decoded::Receipt(typed(binary,bytes)),_=>panic!("Unknown domain case")
    }
}
fn equivalent(a:&Value,b:&Value)->bool {
    match (a,b) {
        (Value::Object(a),Value::Object(b))=>{
            let keep=|k:&str,v:&Value| k!="html" && !v.is_null() && !v.as_array().is_some_and(Vec::is_empty);
            a.iter().filter(|(k,v)|keep(k,v)).all(|(k,v)|b.get(k).is_some_and(|w|equivalent(v,w)))
            && b.iter().filter(|(k,v)|keep(k,v)).all(|(k,v)|a.get(k).is_some_and(|w|equivalent(v,w)))
        },
        (Value::Array(a),Value::Array(b))=>a.len()==b.len() && a.iter().zip(b).all(|(a,b)|equivalent(a,b)),
        (Value::Number(a),Value::Number(b))=>(a.as_f64().unwrap()-b.as_f64().unwrap()).abs()<1e-10,
        _=>a==b
    }
}
fn stats(samples: &[f64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({"medianUs": sorted[sorted.len()/2], "p95Us":sorted[(sorted.len()*95).div_ceil(100)-1], "samplesUs":samples})
}
fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "Build with --release");
    let root = std::env::args().nth(1).expect("capture directory");
    let root = Path::new(&root);
    let manifest: Value = serde_json::from_slice(&std::fs::read(root.join("cases.json"))?)?;
    let mut results = vec![];
    for case in manifest.as_array().unwrap() {
        let name = case.as_str().unwrap();
        let j = std::fs::read(root.join(format!("{name}.json")))?;
        let p = std::fs::read(root.join(format!("{name}.pb")))?;
        ensure!(
            equivalent(&serde_json::from_slice::<Value>(&j)?, &protobuf::decode(&p)?),
            "Parity failed: {name}"
        );
        for _ in 0..64 {
            black_box(decode(name, false, &j));
            black_box(decode(name, true, &p));
        }
        let mut samples = [vec![], vec![]];
        let iterations = 128;
        for round in 0..31 {
            for i in [round % 2, 1 - round % 2] {
                let bytes = if i == 0 { &j } else { &p };
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(decode(name, i == 1, bytes));
                }
                samples[i].push(start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations));
            }
        }
        let mut allocations = vec![];
        for (binary, bytes) in [(false, &j), (true, &p)] {
            CALLS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            TRACK.store(true, Ordering::Relaxed);
            let value = decode(name, binary, bytes);
            TRACK.store(false, Ordering::Relaxed);
            allocations.push(json!({"allocationCalls":CALLS.load(Ordering::Relaxed), "allocatedBytes":BYTES.load(Ordering::Relaxed)}));
            black_box(value);
        }
        results.push(json!({"case":name,"iterationsPerSample":iterations,"json":stats(&samples[0]),"protobuf":stats(&samples[1]),"jsonAllocations":allocations[0],"protobufAllocations":allocations[1]}));
        eprintln!(
            "Decoded {name}: JSON {:.1} us; protobuf {:.1} us",
            results.last().unwrap()["json"]["medianUs"]
                .as_f64()
                .unwrap(),
            results.last().unwrap()["protobuf"]["medianUs"]
                .as_f64()
                .unwrap()
        );
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
