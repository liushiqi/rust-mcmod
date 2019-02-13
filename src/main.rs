#![recursion_limit = "256"]

use std::{error::Error,
          fmt::{Display, Formatter},
          fs::{create_dir_all, OpenOptions},
          io::{stdout, BufReader, Write},
          path::{Path, PathBuf},
          sync::Arc};

use colored::Colorize;
use reqwest::header::USER_AGENT;
use rustyline::{config::Configurer, error::ReadlineError, At, Cmd, Editor, KeyPress, Movement};
use serde::{Deserialize, Serialize};

fn main() {
    let mut reader = Editor::<()>::new();
    let mut dict: Vec<ModInfo> = serde_yaml::from_reader(BufReader::new(
        OpenOptions::new().read(true).create(true).write(true).open("./mods.yaml").unwrap(),
    ))
    .unwrap_or_default();
    while let Err(err) = run(&mut reader, &mut dict) {
        let status = format!("Error: {}", err).red().bold();
        println!("\r\x1b[0K{}", status);
        save(&mut dict).unwrap();
        reader.save_history("history.line").unwrap();
    }
}

fn run(reader: &mut Editor<()>, dict: &mut Vec<ModInfo>) -> Result<(), Box<Error>> {
    dict.sort_by_key(|mod_info| mod_info.id);
    if reader.load_history("history.line").is_err() {
        println!("No previous history.");
    }
    reader.set_auto_add_history(true);
    reader.bind_sequence(KeyPress::ControlLeft, Cmd::Move(Movement::BackwardWord(1, rustyline::Word::Big)));
    reader.bind_sequence(
        KeyPress::ControlRight,
        Cmd::Move(Movement::ForwardWord(1, At::BeforeEnd, rustyline::Word::Big)),
    );
    reader.bind_sequence(KeyPress::Up, Cmd::PreviousHistory);
    reader.bind_sequence(KeyPress::Down, Cmd::NextHistory);

    let invoker = Commands::new();

    loop {
        let line = reader.readline(">> ");
        match line {
            Ok(line) => {
                if !line.is_empty() {
                    let status =
                        invoker.invoke(line.split_whitespace().map(|s| s.to_string()).collect(), dict, reader)?;
                    reader.save_history("history.line").unwrap();
                    if status == Status::QUIT {
                        save(dict)?;
                        break Ok(());
                    }
                    continue;
                }
            },
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => {
                save(dict)?;
                reader.save_history("history.line")?;
                break Ok(());
            },
            Err(err) => {
                let status = format!("Error: {}", err).red().bold();
                println!("\r\x1b[0K{}", status);
                Err(err)?
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DependencyInfo {
    #[serde(alias = "addonId")]
    addon_id: u32,
    r#type: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileInfo {
    id: u32,
    #[serde(alias = "downloadUrl")]
    download_url: String,
    #[serde(alias = "gameVersion")]
    game_version: Vec<String>,
    dependencies: Vec<DependencyInfo>,
    #[serde(alias = "fileNameOnDisk")]
    file_name_on_disk: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionFileInfo {
    #[serde(alias = "gameVersion")]
    game_version: String,
    #[serde(alias = "projectFileId")]
    project_file_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModInfo {
    id: u32,
    name: String,
    #[serde(alias = "websiteUrl")]
    website_url: String,
    summary: String,
    #[serde(alias = "downloadCount")]
    download_count: f64,
    #[serde(alias = "gameVersionLatestFiles")]
    game_version_latest_files: Vec<VersionFileInfo>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Status {
    CONTINUE,
    QUIT,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CommandNotFound {
    command: String,
}

impl CommandNotFound {
    fn new(message: &'_ str) -> Self { CommandNotFound { command: message.to_string() } }
}

impl Display for CommandNotFound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result { write!(f, "Invalid format: {}", self.command) }
}

impl Error for CommandNotFound {
    fn description(&self) -> &str { "not this command." }
}

struct Commands {
    commands: Vec<Arc<Command>>,
}

impl Commands {
    fn new() -> Self {
        let mut commands: Vec<Arc<Command>> = Vec::new();
        commands.push(Arc::new(Save));
        commands.push(Arc::new(Quit));
        commands.push(Arc::new(Download));
        commands.push(Arc::new(Search));
        commands.push(Arc::new(Print));
        commands.push(Arc::new(Update));
        Commands { commands }
    }
}

impl Command for Commands {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        for command in self.commands.clone() {
            let result = command.invoke(line.clone(), dict, editor);
            match result {
                Ok(status) => return Ok(status),
                Err(err) => {
                    if err.description() == "not this command." {
                        continue;
                    } else {
                        Err(err)?;
                    }
                },
            };
        }
        println!("{} {}", "Invalid Command:".red(), line.join(" ").red().italic());
        Ok(Status::CONTINUE)
    }
}

trait Command {
    fn invoke(&self, line: Vec<String>, dict: &mut Vec<ModInfo>, editor: &mut Editor<()>)
        -> Result<Status, Box<Error>>;
}

struct Search;

impl Command for Search {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, _editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        if line.len() > 1 && &line[0] == "search" {
            let client = reqwest::Client::builder().use_rustls_tls().build()?;
            let mod_info: Vec<ModInfo> = client
                .get(&format!(
                    "https://staging_cursemeta.dries007.net/api/v3/direct/addon/search?gameId=432&sectionId=6&searchFilter={}",
                    line[1..].join("%20")
                ))
                .header(USER_AGENT, "liushiqi17@mails.ucas.ac.cn")
                .send()?
                .json()?;
            if !mod_info.is_empty() {
                for mod_info in mod_info {
                    println!(
                        "Mod found, id is {} , name is {}, main page is: {}",
                        mod_info.id.to_string().green(),
                        mod_info.name.blue(),
                        mod_info.website_url.purple().underline()
                    );
                    if dict.iter().find(|info| mod_info.id == info.id).is_none() {
                        dict.push(mod_info);
                    }
                }
                dict.sort_by_key(|mod_info| mod_info.id);
            } else {
                println!("{}", "No mod found.".red());
            }
            Ok(Status::CONTINUE)
        } else {
            Err(Box::from(CommandNotFound::new(&line.join(" "))))
        }
    }
}

struct Download;

impl Command for Download {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        if line.len() > 1 && &line[0] == "download" {
            editor.set_auto_add_history(false);
            let version = loop {
                let line = editor.readline_with_initial("please input the game version to download:", ("1.12", ".2"));
                match line {
                    Ok(line) => break line,
                    Err(_err) => continue,
                }
            };
            editor.set_auto_add_history(true);
            for id in &line[1..] {
                if let Ok(id) = id.parse::<u32>() {
                    if let Some(mod_info) = dict.iter().find(|mod_info| mod_info.id == id) {
                        let dir = format!("./mods/{}/{}", version, mod_info.name);
                        let path = Path::new(&dir).to_path_buf();
                        download_mod_to_dir(&path, id, dict, &version)?;
                    } else {
                        let client = reqwest::Client::builder().use_rustls_tls().build()?;
                        if let Ok(mod_info) = client
                            .get(&format!("https://staging_cursemeta.dries007.net/api/v3/direct/addon/{}", id))
                            .header(USER_AGENT, "liushiqi17@mails.ucas.ac.cn")
                            .send()?
                            .json::<ModInfo>()
                        {
                            let dir = format!("./mods/{}/{}", version, mod_info.name);
                            let path = Path::new(&dir).to_path_buf();
                            dict.push(mod_info);
                            dict.sort_by_key(|mod_info| mod_info.id);
                            download_mod_to_dir(&path, id, dict, &version)?;
                        } else {
                            println!("{} {} {}", "Mod with id".red(), id.to_string().green(), "not found".red());
                        }
                    }
                } else {
                    println!("{} {}", "not valid input:".red(), id.red().bold());
                }
            }
            Ok(Status::CONTINUE)
        } else {
            Err(Box::from(CommandNotFound::new(&line.join(" "))))
        }
    }
}

struct Print;

impl Command for Print {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, _editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        if line.len() > 1 && &line[0] == "print" {
            for id in &line[1..] {
                if let Ok(id) = id.parse::<u32>() {
                    if let Some(mod_info) = dict.iter().find(|mod_info| mod_info.id == id) {
                        println!("{:#?}", mod_info);
                    } else {
                        let client = reqwest::Client::builder().use_rustls_tls().build()?;
                        if let Ok(mod_info) = client
                            .get(&format!("https://staging_cursemeta.dries007.net/api/v3/direct/addon/{}", id))
                            .header(USER_AGENT, "liushiqi17@mails.ucas.ac.cn")
                            .send()?
                            .json::<ModInfo>()
                        {
                            println!("{:#?}", mod_info);
                            dict.push(mod_info);
                            dict.sort_by_key(|mod_info| mod_info.id);
                        } else {
                            println!("{} {} {}", "Mod with id".red(), id.to_string().red().bold(), "not found".red());
                        }
                    }
                } else {
                    println!("{} {}", "not valid input:".red(), id.red());
                }
            }
            Ok(Status::CONTINUE)
        } else {
            Err(Box::from(CommandNotFound::new(&line.join(" "))))
        }
    }
}

struct Update;

impl Command for Update {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, _editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        if !line.is_empty() && (&line[0] == "update" || &line[0] == "clear") {
            dict.clear();
            save(dict)?;
            Ok(Status::CONTINUE)
        } else {
            Err(Box::from(CommandNotFound::new(&line.join(" "))))
        }
    }
}

struct Save;

impl Command for Save {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        if !line.is_empty() && &line[0] == "save" {
            save(dict)?;
            editor.save_history("history.line").unwrap();
            Ok(Status::CONTINUE)
        } else {
            Err(Box::from(CommandNotFound::new(&line.join(" "))))
        }
    }
}

struct Quit;

impl Command for Quit {
    fn invoke(
        &self, line: Vec<String>, dict: &mut Vec<ModInfo>, editor: &mut Editor<()>,
    ) -> Result<Status, Box<Error>> {
        if !line.is_empty() && (&line[0] == "quit" || &line[0] == "exit") {
            save(dict)?;
            editor.save_history("history.line").unwrap();
            Ok(Status::QUIT)
        } else {
            Err(Box::from(CommandNotFound::new(&line.join(" "))))
        }
    }
}

fn save(dict: &mut Vec<ModInfo>) -> Result<(), Box<Error>> {
    let file = OpenOptions::new().write(true).append(false).create(true).open("./mods.yaml")?;
    file.set_len(0)?;
    serde_yaml::to_writer(file, dict)?;
    Ok(())
}

fn download_mod_to_dir(dir: &PathBuf, id: u32, dict: &mut Vec<ModInfo>, version: &str) -> Result<(), Box<Error>> {
    let mut stack = vec![id];
    let mut downloaded = Vec::<u32>::default();
    loop {
        if let Some(id) = stack.pop() {
            if !downloaded.contains(&id) {
                if let Some(mod_info) = dict.iter().find(|mod_info| mod_info.id == id) {
                    let file_info =
                        mod_info.game_version_latest_files.iter().find(|file_info| file_info.game_version == version);
                    if let Some(file_info) = file_info {
                        create_dir_all(dir)?;
                        let client = reqwest::Client::builder().use_rustls_tls().build()?;
                        let file_info: FileInfo = client
                            .get(&format!(
                                "https://staging_cursemeta.dries007.net/api/v3/direct/addon/{}/file/{}",
                                id, file_info.project_file_id
                            ))
                            .header(USER_AGENT, "liushiqi17@mails.ucas.ac.cn")
                            .send()?
                            .json()?;
                        print!("Downloading {}", file_info.file_name_on_disk.green());
                        stdout().flush()?;
                        download(&file_info.download_url, &dir.join(file_info.file_name_on_disk.clone()))?;
                        println!(
                            "\r\x1b[0KDownload {} from {} succeed!",
                            file_info.file_name_on_disk.green(),
                            file_info.download_url.purple().underline()
                        );
                        downloaded.push(id);
                        for dep in file_info.dependencies.iter() {
                            if dep.r#type == 1 || dep.r#type == 3 {
                                stack.push(dep.addon_id);
                            }
                        }
                    } else {
                        let message =
                            format!("No mod {} for Minecraft version {}.", mod_info.name.bold(), version.italic())
                                .red();
                        println!("{}", message);
                    }
                } else {
                    let client = reqwest::Client::builder().use_rustls_tls().build()?;
                    let mod_info: ModInfo = client
                        .get(&format!("https://staging_cursemeta.dries007.net/api/v3/direct/addon/{}", id))
                        .header(USER_AGENT, "liushiqi17@mails.ucas.ac.cn")
                        .send()?
                        .json()?;
                    stack.push(mod_info.id);
                    if dict.iter().find(|info| mod_info.id == info.id).is_none() {
                        dict.push(mod_info);
                    }
                }
            }
        } else {
            break Ok(());
        }
    }
}

fn download(url: &str, write_to: &PathBuf) -> Result<(), Box<Error>> {
    let client = reqwest::Client::builder().use_rustls_tls().timeout(None).build()?;
    client
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.81 Safari/537.36",
        )
        .send()?
        .copy_to(&mut OpenOptions::new().write(true).create(true).append(false).open(write_to)?)?;
    Ok(())
}
