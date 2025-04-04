use bitcoin::{
    blockdata::locktime::absolute::LockTime as AbsoluteLockTime,
    Amount,
    consensus::encode::deserialize_hex,
    hashes::Hash,
    OutPoint,
    ScriptBuf,
    Sequence,
    consensus::encode::serialize_hex,
    Transaction,
    Txid,
    TxIn,
    TxOut,
    blockdata::transaction::Version,
    Witness,
};

use bitcoincore_rpc::{
    Client,
    RpcApi,
    Auth,
};

use clap::Parser;

use rand::{
    RngCore,
    SeedableRng,
};

use rand_chacha::ChaCha20Rng;

use serde::Serialize;

use std::cmp::max;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::str::FromStr;

/// Generate a random integer in a given range
fn random_range<R: RngCore>(rand: &mut R, range: &RangeInclusive<usize>) -> usize {
    let x = rand.next_u64() as usize;
    let size = max(range.end() - range.start(), 0) + 1;

    range.start() + (x % size)
}

const INPUT_COUNT: RangeInclusive<usize> = 1..=129;
const OUTPUT_COUNT: RangeInclusive<usize> = 0..=129;

const SCRIPT_PUBKEY_LENGTH: RangeInclusive<usize> = 0..=129;
const SCRIPT_SIG_LENGTH: RangeInclusive<usize> = 0..=129;
const WITNESS_LENGTH: RangeInclusive<usize> = 0..=129;
const WITNESS_ITEM_LENGTH: RangeInclusive<usize> = 0..=520;

/// An approximate amount of random bytes in the transaction
/// Note that this doesn't account for things like VarInt lengths
const RANDOM_BYTES_COUNT: RangeInclusive<usize> = 0..=10_000;

/// Generate a random number of random bytes, no more than max_bytes
fn random_bytes_lt<R: RngCore>(rand: &mut R, length: &RangeInclusive<usize>, max_bytes: &mut usize) -> Vec<u8> {
    let mut result = Vec::new();

    if *max_bytes < 1 {
        return result;
    }

    let length = random_range(rand, length) % (*max_bytes + 1);

    *max_bytes = max_bytes.saturating_sub(length);

    result.resize(length, 0);

    rand.fill_bytes(result.as_mut());

    result
}

fn random_witness_item<R: RngCore>(rand: &mut R, max_bytes: &mut usize) -> Vec<u8> {
    random_bytes_lt(rand, &WITNESS_ITEM_LENGTH, max_bytes)
}

fn random_tx<R: RngCore>(rand: &mut R) -> Transaction {
    let version = Version::non_standard(rand.next_u32() as i32);
    let lock_time = AbsoluteLockTime::from_consensus(rand.next_u32());

    let input_count = random_range(rand, &INPUT_COUNT);
    let output_count = random_range(rand, &OUTPUT_COUNT);

    let mut random_bytes_remaining = random_range(rand, &RANDOM_BYTES_COUNT);

    let has_witness = (rand.next_u32() % 2) == 1;

    // Generate inputs
    let mut input: Vec<TxIn> = Vec::new();
    for _ in 0..input_count {
        let mut txid = [0u8; 32];
        rand.fill_bytes(txid.as_mut());

        let previous_output = OutPoint {
            txid: Txid::hash(txid.as_ref()),
            vout: rand.next_u32(),
        };

        random_bytes_remaining = random_bytes_remaining.saturating_sub(36);

        let mut witness = Witness::new();

        if has_witness {
            let witness_item_count = random_range(rand, &WITNESS_LENGTH);

            for _ in 0..witness_item_count {
                let witness_item = random_witness_item(rand, &mut random_bytes_remaining);
                witness.push(witness_item);

                if random_bytes_remaining < 1 {
                    break;
                }
            }
        }

        let script_sig = if has_witness {
            ScriptBuf::from_bytes(random_bytes_lt(rand, &SCRIPT_SIG_LENGTH, &mut random_bytes_remaining))
        } else {
            ScriptBuf::new()
        };

        input.push(TxIn {
            previous_output,
            script_sig,
            sequence: Sequence::from_consensus(rand.next_u32()),
            witness,
        });

        if random_bytes_remaining < 1 {
            break;
        }
    }

    // Generate outputs
    let sats_modulus = Amount::MAX_MONEY.to_sat() + 1;
    let mut output: Vec<TxOut> = Vec::new();
    for _ in 0..output_count {
        let value = Amount::from_sat(rand.next_u64() % sats_modulus);

        let script_pubkey_bytes = random_bytes_lt(rand, &SCRIPT_PUBKEY_LENGTH, &mut random_bytes_remaining);

        output.push(TxOut {
            value,
            script_pubkey: ScriptBuf::from_bytes(script_pubkey_bytes),
        });

        if random_bytes_remaining < 1 {
            break;
        }
    }

    Transaction {
        version,
        lock_time,
        input,
        output,
    }
}

/// `Write`-able output sink for either stdout or a filesystem file
enum OutputDestination {
    Stdout(std::io::Stdout),
    File(std::fs::File),
}

impl std::io::Write for OutputDestination {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            OutputDestination::Stdout(stdout) => stdout.write(buf),
            OutputDestination::File(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            OutputDestination::Stdout(stdout) => stdout.flush(),
            OutputDestination::File(file) => file.flush(),
        }
    }
}

impl std::str::FromStr for OutputDestination {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "-" {
            Ok(Self::Stdout(std::io::stdout()))
        } else {
            let out_path = PathBuf::from_str(s)
                .expect("infallible");

            std::fs::File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(out_path)
                .map(|file| Self::File(file))
        }
    }
}

#[derive(Debug, Serialize)]
struct Desc {
    #[serde(rename = "Inputs")]
    inputs: u32,

    #[serde(rename = "Outputs")]
    outputs: u32,

    #[serde(rename = "Witness")]
    witness: bool,

    #[serde(rename = "Version")]
    version: i32,

    #[serde(rename = "scriptSigs")]
    script_sigs: bool,
}

#[derive(Debug, Serialize)]
struct CtvTestVector {
    #[serde(rename = "hex_tx")]
    transaction: String,

    spend_index: Vec<u32>,

    result: Vec<String>,

    desc: Desc,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum CtvTestVectorEntry {
    TestVector(CtvTestVector),
    Documentation(String),
}

#[derive(Parser)]
struct CommandLineArguments {
    #[arg(short = 'u', long = "rpc-url")]
    url: String,

    #[arg(short = 'c', long = "cookie-file")]
    cookie: PathBuf,

    #[arg(short = 'n', long = "transaction-count", default_value = "100")]
    transaction_count: usize,

    #[arg(short = 'o', long = "out-file", default_value = "-")]
    out_path: String,
}

fn main() {
    let args = CommandLineArguments::parse();

    let cookie = Auth::CookieFile(args.cookie.clone());
    let client = Client::new(args.url.as_ref(), cookie)
        .expect("open client");

    let mut rng = ChaCha20Rng::from_os_rng();

    let out = OutputDestination::from_str(args.out_path.as_ref()).expect("Open out");

    let mut entries = Vec::new();

    entries.push(CtvTestVectorEntry::Documentation(
        "{\"hex_tx\":string (hex tx), \"spend_index\":[number], \"result\": [string (hex hash)]}"
            .to_string()
    ));

    for _n in 0..args.transaction_count {
        let tx = random_tx(&mut rng);

        let mut spend_index: Vec<u32> = vec![0, 1];
        spend_index.extend((0..2).map(|_| rng.next_u32()));

        let mut result: Vec<String> = Vec::new();

        let hextx = serialize_hex(&tx);

        let _deserialized_hex: Transaction = deserialize_hex(&hextx)
            .expect("deserialize hex");

        let desc = Desc {
            inputs: tx.input.len() as u32,
            outputs: tx.output.len() as u32,
            witness: tx.input.iter().any(|input| !input.witness.is_empty()),
            version: tx.version.0,
            script_sigs: tx.input.iter().any(|input| !input.script_sig.is_empty()),
        };

        for i in spend_index.iter() {
            let default_template: String = client.call("getdefaulttemplate", &[
                 hextx.clone().into(),
                 (*i).into(),
                 if desc.witness { true.into() } else { false.into() },
            ]).unwrap();

            result.push(default_template);
        }

        entries.push(CtvTestVectorEntry::TestVector(
            CtvTestVector {
                transaction: hextx,
                spend_index,
                result,
                desc,
            }
        ));
    }

    serde_json::to_writer_pretty(out, &entries)
        .expect("write json");
}
