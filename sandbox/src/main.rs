mod application;
mod camera;
mod constants;
mod input_manager;
mod result;
mod settings;

use application::*;
use camera::*;
use input_manager::*;
use result::*;
use settings::*;

use std::str::FromStr;

use winit::{event_loop::EventLoop, raw_window_handle::HasDisplayHandle};

const DEFAULT_SETTINGS: &str = include_str!("../../files/default_settings.yaml");

fn main() -> Result<()> {
    {
        const LEVEL: tracing::Level = if cfg!(debug_assertions) {
            tracing::Level::DEBUG
        } else {
            tracing::Level::ERROR
        };

        tracing_subscriber::fmt()
            .with_max_level(LEVEL)
            .with_file(true)
            .with_line_number(true)
            .init();
    }

    let args: Box<[String]> = std::env::args().collect();
    let name = format!(
        "{}",
        std::env::current_exe()?.file_name().unwrap().display()
    );

    let print_usage = || -> Result<()> {
        println!(
            "Invalid program arguments. Usage: {} <options> <model>",
            name.clone()
        );
        println!("To view all options type {} --help", name);
        return Ok(());
    };

    if args.len() < 2 {
        return print_usage();
    }

    if let Some(_) = args.iter().find(|s| s.as_str() == "--help") {
        println!("Options:");
        println!(
            "    --settings. This is an optional argument. Defaults to files/default_settings.yaml when unspecified."
        );
        return Ok(());
    }

    let model_path = {
        let args: Vec<String> = std::env::args().collect();
        std::path::PathBuf::from(args[args.len() - 1].clone())
    };

    let settings_dir = if let Some(dirs) = directories::ProjectDirs::from("", "", &name) {
        dirs.config_dir().to_path_buf()
    } else {
        println!("Could not find config directory!");
        return Ok(());
    };

    // ensure that default_settings.yaml exists
    {
        let settings_path = settings_dir.join("default_settings.yaml");

        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if !settings_path.exists() {
            std::fs::write(settings_path, DEFAULT_SETTINGS)?;
        }
    }

    let settings =
        {
            let arg_idx = args.iter().enumerate().find_map(|(idx, arg)| {
                if arg == "--settings" { Some(idx) } else { None }
            });

            let path_str = if let Some(idx) = arg_idx {
                args.get(idx + 1)
            } else {
                Some(&String::from_str("default_settings.yaml").unwrap())
            };

            if let Some(str) = path_str {
                let path = settings_dir.join(str);

                Settings::new(&path, &args)?
            } else {
                println!("Settings file not present!");
                return Ok(());
            }
        };

    let event_loop = EventLoop::new().inspect_err(|e| tracing::error!("{e}"))?;

    let name = model_path.display().to_string();

    let mut app = {
        const DEBUG_ENABLED: bool = cfg!(debug_assertions);
        let owned_display_handle = event_loop.owned_display_handle();
        let display_handle = owned_display_handle.display_handle()?;
        Application::new(
            name.into_boxed_str(),
            settings,
            model_path.as_path(),
            DEBUG_ENABLED,
            &display_handle,
        )?
    };

    event_loop
        .run_app(&mut app)
        .inspect_err(|e| tracing::error!("{e}"))?;

    Ok(())
}
