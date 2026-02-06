use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use csv::Writer;
use lru_sim::update_hit_miss_csv;

const BLOCK_SIZE: u32 = 16;

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub block_tag: u32,
    pub forward_ri: i32,
}

pub fn convert(in_file_path: &PathBuf, data_path: PathBuf) -> Result<Vec<TraceEntry>, Box<dyn std::error::Error>> {
    print!("trace_path: {:?} ------ ", in_file_path);
    let mut file = BufReader::new(File::open(&in_file_path)?);

    let mut buffer = [0u8; 9]; // pc (4) + addr (4) + hit (1)

    let mut block_access_indices: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut accesses: Vec<(u32, u32, i32, bool, bool)> = Vec::new();

    let mut clock_time = 1;
    let mut hit_trace: Vec<bool> = Vec::new();

    while file.read_exact(&mut buffer).is_ok() {
        let pc = u32::from_le_bytes(buffer[0..4].try_into()?);
        let addr = u32::from_le_bytes(buffer[4..8].try_into()?);
        let is_hit = buffer[8] != 0;

        let block_tag = addr / BLOCK_SIZE;
        let last_element_of_block = (addr % BLOCK_SIZE) == (BLOCK_SIZE - 1);

        hit_trace.push(is_hit);
        accesses.push((pc, block_tag, clock_time, is_hit, last_element_of_block));
        block_access_indices.entry(block_tag).or_default().push(accesses.len() - 1);
        clock_time += 1;
    }

    // Forward RI calculation
    let mut forward_ri: Vec<i32> = vec![i32::MAX; accesses.len()];
    for indices in block_access_indices.values() {
        for w in indices.windows(2) {
            let curr = w[0];
            let next = w[1];
            let dt = accesses[next].2 - accesses[curr].2;
            forward_ri[curr] = dt;
        }
    }

    // Output CSV for compatibility
    std::fs::create_dir_all(&data_path)?;
    let output_path = data_path.join("block_trace.csv");
    let output_file = File::create(&output_path)?;
    let mut wtr = Writer::from_writer(output_file);
    wtr.write_record(&["phase_id_ref", "forward_ri", "tag", "last_element"])?;

    let mut entries: Vec<TraceEntry> = Vec::with_capacity(accesses.len());
    for (i, (inst_ptr, block_tag, _clock_time, _is_hit, last_element)) in accesses.iter().enumerate() {
        let entry = TraceEntry {
            block_tag: *block_tag,
            forward_ri: forward_ri[i],
        };
        entries.push(entry);
        wtr.write_record(&[
            format!("{:x}", inst_ptr),
            format!("{:x}", forward_ri[i]),
            format!("{:x}", *block_tag),
            format!("{}", if *last_element { 1 } else { 0 }),
        ])?;
    }

    wtr.flush()?;
    let policy_name = format!("CLAM_{}", 128);
    // update_hit_miss_csv(data_path.as_ref(), &policy_name, &hit_trace).unwrap();
    println!("Block Trace with Forward RI Completed.");
    Ok(entries)
}
