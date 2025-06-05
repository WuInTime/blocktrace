use csv::ReaderBuilder;
use csv::Writer;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use lru_sim::update_hit_miss_csv;

const BLOCK_SIZE: usize = 16; // Size of a block in words
                              // const BLOCK_SIZE: usize = 64; // Size of a block in bytes

pub fn convert(in_file_path: PathBuf, data_path: PathBuf) -> Result<Vec<usize>, Box<dyn Error>> {
    // let trace_path = format!("{}{}", data_path, in_file_path);
    let trace_path = in_file_path.clone();
    print!("trace_path: {:?} ------ ", trace_path);

    // let output_path = format!("{:?}/block_trace.csv", data_path.join(in_file_path.file_stem().unwrap()));
    let output_path = data_path
        // .join(in_file_path.file_stem().unwrap())
        .join("block_trace.csv");

    let input_file = File::open(&trace_path)?;
    let parent_dir = output_path.parent().unwrap();
    create_dir_all(parent_dir)?;
    let output_file = File::create(&output_path)?;

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(input_file);
    // println!("rdr lines: {}", rdr.records().count());
    let mut wtr = Writer::from_writer(output_file);

    wtr.write_record(&["phase_id_ref", "backward_ri", "tag", "time"])?;

    let mut clock_time: usize = 1;
    let mut last_access_map: HashMap<usize, usize> = HashMap::new();
    let mut block_trace: Vec<usize> = Vec::new();
    let mut hit_trace: Vec<u8> = Vec::new();

    for result in rdr.records() {
        let record = result?;

        // Extract the fields from the row
        // let _operation = &record[1]; // "w" or "r"
        // let _instruction_pointer = usize::from_str_radix(&record[0].trim_start_matches("0x"), 16)?;
        // let address = usize::from_str_radix(&record[1].trim_start_matches("0x"), 16)?;
        let instruction_pointer = usize::from_str_radix(&record[0], 16)?; // 16 means hexadecimal
        let address = usize::from_str_radix(&record[1], 16)?;
        let _is_hit = record[2].trim() == "1"; // Assuming the third column indicates hit (1) or miss (0)


        // Calculate the block tag (RIT using 64-byte blocks)
        let block_tag = address / BLOCK_SIZE;
        block_trace.push(block_tag);
        hit_trace.push(_is_hit as u8);
        let reuse_interval = last_access_map
            .insert(block_tag, clock_time)
            .map_or(i32::MAX, |last_time| (clock_time - last_time) as i32);

        wtr.write_record(&[
            // phase_id_ref.to_string(),
            format!("{:x}", instruction_pointer),
            format!("{:x}", reuse_interval),
            format!("{:x}", block_tag as u32),
            clock_time.to_string(),
        ])?;

        clock_time += 1;
    }

    update_hit_miss_csv(&data_path, "PLRU_64", &hit_trace)?;
    wtr.flush()?;
    println!("Block Trace Completed.");
    // read_third_column_as_usize_vec(&output_path.to_str().unwrap())
    Ok(block_trace)
}
