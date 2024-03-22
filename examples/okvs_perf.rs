use clap::Parser;
use bpsy23::{
    bpsy23::BPSY23, okvs::{OkvsDecoder, OkvsEncoder},
    block::Block, hash::BufferedRandomGenerator,
    utils::TimerOnce,
    utils::print_communication
};

#[derive(Parser, Debug)]
struct Arguments {

    #[arg(short, long, default_value_t = 65536)]
    n: usize,
    #[arg(short, long, default_value_t = 0.0)]
    epsilon: f64,
    #[arg(short = 't', long, default_value_t = 65536)]
    num_threads: usize,
    
    /// See the Appendix F of BPSY23 paper for the choice of width.
    /// eps = 0.03, n = 65536, width = 570
    /// eps = 0.03, n = 2^20, width = 612
    #[arg(short, long, default_value_t = 570)]
    width: usize,
}

fn test_encoder<E>(args: Arguments, encoder: E) where
    E: OkvsEncoder<Block, Block> + OkvsDecoder<Block, Block>
{
    let mut map = Vec::new();
    let mut rng = BufferedRandomGenerator::from_entropy();
    for _ in 0..args.n {
        let key = rng.gen_block();
        let value = rng.gen_block();
        map.push((key, value));
    }

    let timer = TimerOnce::new();
    let s = encoder.encode(&map);
    timer.finish("Encode time");

    let keys = map.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
    let values = encoder.decode_many(&s, &keys);
    
    let timer = TimerOnce::new();
    let decoded = encoder.decode_many(&s, &keys);
    timer.finish("Decode time");
    assert_eq!(decoded, values, "decoded = {:?}, values = {:?}", decoded, values);
    print_communication("Encoded length", 0, s.len() * std::mem::size_of::<Block>(), 1);
}

fn test_bpsy23(mut args: Arguments) {

    println!("[BPSY23 arguments]");
    if args.epsilon == 0.0 {
        args.epsilon = 0.03;
        println!("  eps   = {} (default)", args.epsilon);
    } else {
        println!("  eps   = {}", args.epsilon);
    }
    println!("  width = {}", args.width);
    
    let encoder = BPSY23::new(args.epsilon, args.width);
    test_encoder(args, encoder);
}


fn main() {
    let args = Arguments::parse();
    let mut map = Vec::new();
    let mut rng = BufferedRandomGenerator::from_entropy();
    for _ in 0..args.n {
        let key = rng.gen_block();
        let value = rng.gen_block();
        map.push((key, value));
    }

    println!("[Arguments]");
    println!("  Set size (n)   = {}", args.n);
    println!("    log n        = {:.1}", (args.n as f64).log2());

    test_bpsy23(args);
}


