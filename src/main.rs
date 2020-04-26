use once_cell::sync::OnceCell;
use regex::Regex;
use std::{
    env,
    error::Error,
    fs, io,
    iter::Sum,
    ops::Add,
    path::{Path, PathBuf},
};

// == REGEX ==
// resources in kuber config look like this:
// // requests:
//     cpu: "100m"
//     memory: "128M"
// limits:
//     memory: "512M"
//     cpu: "500m"
//
// below are regex to parse all combinations
static REGEX_REQ_CPU_MEM: OnceCell<Regex> = OnceCell::new();
static REGEX_REQ_MEM_CPU: OnceCell<Regex> = OnceCell::new();
static REGEX_LIM_CPU_MEM: OnceCell<Regex> = OnceCell::new();
static REGEX_LIM_MEM_CPU: OnceCell<Regex> = OnceCell::new();

// resource syntax, e.g. 500m or 512Mi or 512M
static REGEX_NUMERIC_PREFIX: OnceCell<Regex> = OnceCell::new();

fn main() -> Result<(), Box<dyn Error>> {
    let dir = match env::args().skip(1).next() {
        Some(dir) => PathBuf::from(dir),
        None => env::current_dir()?,
    };

    println!("Analyzing kubernetes configs in {:?}", dir);

    REGEX_REQ_CPU_MEM
        .set(Regex::new(
            "requests:\\s*cpu:\\s*\"(.*)\"\\s*memory:\\s*(.*)",
        )?)
        .unwrap();

    REGEX_REQ_MEM_CPU
        .set(Regex::new(
            "requests:\\s*memory:\\s*\"(.*)\"\\s*cpu:\\s*\"(.*)\"",
        )?)
        .unwrap();

    REGEX_LIM_CPU_MEM
        .set(Regex::new(
            "limits:\\s*cpu:\\s*\"(.*)\"\\s*memory:\\s*(.*)",
        )?)
        .unwrap();

    REGEX_LIM_MEM_CPU
        .set(Regex::new(
            "limits:\\s*memory:\\s*\"(.*)\"\\s*cpu:\\s*\"(.*)\"",
        )?)
        .unwrap();

    REGEX_NUMERIC_PREFIX.set(Regex::new("^([0-9]+)")?).unwrap();

    let resources: Resources = find_yamls(&dir)?
        .iter()
        .map(|p| fs::read_to_string(p))
        .filter_map(|r| r.ok())
        .map(|c| analyze(&c))
        .sum();

    println!("Total resources: {:?}", resources);

    Ok(())
}

fn find_yamls(root_dir: &Path) -> Result<Vec<PathBuf>, io::Error> {
    if !root_dir.is_dir() {
        Ok(Vec::new())
    } else {
        let mut dirs_stack = vec![PathBuf::from(root_dir)];
        let mut yamls = vec![];

        while let Some(dir) = dirs_stack.pop() {
            // traverse this dir, push all other dirs to stack, push all found yamls
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    dirs_stack.push(path);
                } else if let Some(ext) = path.extension() {
                    if ext == "yaml" {
                        yamls.push(path);
                    }
                }
            }
        }

        Ok(yamls)
    }
}

#[derive(Debug)]
struct Resources {
    mem_request: u64,
    mem_limit: u64,
    cpu_request: f32,
    cpu_limit: f32,
}

impl Sum for Resources {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(
            Resources {
                mem_request: 0,
                mem_limit: 0,
                cpu_request: 0.0,
                cpu_limit: 0.0,
            },
            Add::add,
        )
    }
}

impl Add for Resources {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            mem_request: self.mem_request + other.mem_request,
            mem_limit: self.mem_limit + other.mem_limit,
            cpu_request: self.cpu_request + other.cpu_request,
            cpu_limit: self.cpu_limit + other.cpu_limit,
        }
    }
}

fn analyze(config: &str) -> Resources {
    let mut mem_requests_str = Vec::new();
    let mut cpu_requests_str = Vec::new();
    let mut mem_limits_str = Vec::new();
    let mut cpu_limits_str = Vec::new();

    for cap in REGEX_REQ_CPU_MEM.get().unwrap().captures_iter(config) {
        cpu_requests_str.push(String::from(&cap[1]));
        mem_requests_str.push(String::from(&cap[2]));
    }

    for cap in REGEX_REQ_MEM_CPU.get().unwrap().captures_iter(config) {
        mem_requests_str.push(String::from(&cap[1]));
        cpu_requests_str.push(String::from(&cap[2]));
    }

    for cap in REGEX_LIM_CPU_MEM.get().unwrap().captures_iter(config) {
        cpu_limits_str.push(String::from(&cap[1]));
        mem_limits_str.push(String::from(&cap[2]));
    }

    for cap in REGEX_LIM_MEM_CPU.get().unwrap().captures_iter(config) {
        mem_limits_str.push(String::from(&cap[1]));
        cpu_limits_str.push(String::from(&cap[2]));
    }

    Resources {
        mem_request: mem_requests_str.iter().map(|s| parse_mem(s)).sum(),
        mem_limit: mem_limits_str.iter().map(|s| parse_mem(s)).sum(),
        cpu_request: cpu_requests_str.iter().map(|s| parse_cpu(s)).sum(),
        cpu_limit: cpu_limits_str.iter().map(|s| parse_cpu(s)).sum(),
    }
}

fn num_prefix_or_zero(s: &str) -> u64 {
    match REGEX_NUMERIC_PREFIX
        .get()
        .unwrap()
        .captures_iter(s)
        .map(|cap| cap[1].parse())
        .next()
    {
        Some(Ok(v)) => v,
        _ => 0,
    }
}

fn parse_mem(s: &str) -> u64 {
    num_prefix_or_zero(s)
}

fn parse_cpu(s: &str) -> f32 {
    num_prefix_or_zero(s) as f32 / 1000.0
}
