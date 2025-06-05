use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use csv::{ReaderBuilder, Writer};

const BLOCK_SIZE: usize = 16;

pub fn convert(in_file_path: PathBuf, data_path: PathBuf) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
    let input_file = File::open(&in_file_path)?;
    print!("trace_path: {:?} ------ ", in_file_path);
    let mut rdr = ReaderBuilder::new().has_headers(false).from_reader(input_file);

    // For each block, a vector of all access positions (indices into `accesses`)
    let mut block_access_indices: HashMap<usize, Vec<usize>> = HashMap::new();
    // All accesses: (inst_ptr, block_tag, clock_time, is_hit)
    let mut accesses: Vec<(usize, usize, usize, u8)> = Vec::new();

    let mut clock_time = 1;
    for result in rdr.records() {
        let record = result?;
        let inst_ptr = usize::from_str_radix(&record[0], 16)?;
        let address = usize::from_str_radix(&record[1], 16)?;
        let is_hit = record[2].trim() == "1";
        let block_tag = address / BLOCK_SIZE;
        accesses.push((inst_ptr, block_tag, clock_time, is_hit as u8));
        block_access_indices.entry(block_tag).or_default().push(accesses.len() - 1);
        clock_time += 1;
    }

    // For each access, determine forward reuse interval
    // forward_ri[i] = accesses[next_index].clock_time - accesses[i].clock_time
    let mut forward_ri: Vec<i32> = vec![i32::MAX; accesses.len()];
    for indices in block_access_indices.values() {
        for w in indices.windows(2) {
            let curr = w[0];
            let next = w[1];
            let dt = (accesses[next].2 - accesses[curr].2) as i32;
            forward_ri[curr] = dt;
        }
        // Last access to each block remains i32::MAX (never accessed again)
    }

    // Output as desired
    let output_path = data_path.join("block_trace.csv");
    let output_file = File::create(&output_path)?;
    let mut wtr = Writer::from_writer(output_file);
    wtr.write_record(&["phase_id_ref", "forward_ri", "tag", "time"])?;
    // TODO: It's actually `forward_ri`, but keeping the original name for compatibility

    for (i, (inst_ptr, block_tag, clock_time, _is_hit)) in accesses.iter().enumerate() {
        wtr.write_record(&[
            format!("{:x}", inst_ptr),
            format!("{:x}", forward_ri[i]),
            format!("{:x}", *block_tag as u32),
            format!("{}", clock_time),
        ])?;
    }
    wtr.flush()?;
    println!("Block Trace with Forward RI Completed.");
    Ok(accesses.iter().map(|(_, block_tag, _, _)| *block_tag).collect())
}
