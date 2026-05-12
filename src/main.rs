use std::time::Instant;

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

/// Path of Sled DB in disk.
pub const SLED_PATH: &str = "./sled";

/// Path to RocksDB in disk.
pub const ROCKS_PATH: &str = "./rocks";

/// This is the concatenation merge operator in Sled.
/// Sled 1.0-alpha removed the dedicated merge-operator API, so we emulate it via
/// `update_and_fetch` (atomic read-modify-write) using this function as the update closure.
fn sled_cat(val: Option<&[u8]>, new: &[u8]) -> Option<Vec<u8>> {
    Some(val.into_iter().flatten().chain(new).cloned().collect())
}

/// This is the concatenation merge operator in RocksDB.
fn rocks_cat(_key: &[u8], val: Option<&[u8]>, new: &rocksdb::MergeOperands) -> Option<Vec<u8>> {
    Some(
        val.into_iter()
            .flatten()
            .chain(new.into_iter().flatten())
            .cloned()
            .collect(),
    )
}

/// Quick and dirty slice to u32.
fn from_bytes(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn main() {
    // This is how we initialize Sled.
    // Sled 1.0-alpha simplified initialization: no ConfigBuilder tuning knobs, no merge operator
    // registration, and compression/zstd are unconditional (no feature flag to toggle).
    let sled_db: sled::Db = sled::open(SLED_PATH).unwrap();

    // This is how we initialize RocksDB.
    let rocks_db = {
        let mut options = rocksdb::Options::default();
        options.create_if_missing(true);
        options.set_merge_operator_associative("rocks_cat", rocks_cat);
        options.set_compression_type(rocksdb::DBCompressionType::Lz4);

        rocksdb::DB::open(&options, ROCKS_PATH).unwrap()
    };

    // 1. Fill each DB with consecutive integers, all holding ntegers from 0 to 9 concatenated.

    // This is how we do it in Sled.
    let tic = Instant::now();

    for i in 0..4_000_000u32 {
        for j in 0..10u32 {
            sled_db
                .update_and_fetch(&i.to_le_bytes(), |old| sled_cat(old, &j.to_le_bytes()))
                .unwrap();
        }
    }

    println!("Sled: {:?}", tic.elapsed());

    // This is how we do it in RocksDB.
    let tic = Instant::now();
    for i in 0..4_000_000u32 {
        for j in 0..10u32 {
            rocks_db.merge(&i.to_le_bytes(), &j.to_le_bytes()).unwrap();
        }
    }

    println!("RocksDB: {:?}", tic.elapsed());

    // 2. Now, sum all integers contained in all keys.

    // This is how we do it in Sled.
    let tic = Instant::now();
    let count = sled_db
        .iter()
        .map(Result::unwrap)
        .map(|(_, val)| val.as_ref().windows(4).map(from_bytes).collect::<Vec<_>>())
        .flatten()
        .map(|i| i as u64)
        .sum::<u64>();
    dbg!(count);

    println!("Sled: {:?}", tic.elapsed());

    // This is how we do it in RocksDB.
    let tic = Instant::now();
    let count = rocks_db
        .iterator(rocksdb::IteratorMode::Start)
        .map(Result::unwrap)
        .map(|(_, val)| val.as_ref().windows(4).map(from_bytes).collect::<Vec<_>>())
        .flatten()
        .map(|i| i as u64)
        .sum::<u64>();
    dbg!(count);

    println!("RocksDB: {:?}", tic.elapsed());
}
