use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage:\n  shiftninja-cli list <files…>\n  shiftninja-cli run <action> <files…>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") if args.len() > 1 => {
            let paths: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();
            for a in engine::all_actions_for(&paths) {
                println!("{:<8} {:<16} {}", format!("{:?}", a.wheel).to_lowercase(), a.id, a.title);
            }
            ExitCode::SUCCESS
        }
        Some("run") if args.len() > 2 => {
            let paths: Vec<PathBuf> = args[2..].iter().map(PathBuf::from).collect();
            match engine::run(&args[1], &paths) {
                Ok(created) => {
                    for p in created {
                        println!("{}", p.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("shiftninja: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
