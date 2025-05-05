use serenity::{
    async_trait,
    model::{
        channel::Message,
        gateway::Ready,
        id::GuildId,
    },
    prelude::*,
    utils::{content_safe, ContentSafeOptions},
};
use std::{
    collections::HashMap,
    env,
    fs::{File, OpenOptions},
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::sleep;
use reqwest; // Requires `reqwest = { version = "0.11", features = ["json"] }`

// Structure to hold custom commands
struct CustomCommands {
    commands: Arc<Mutex<HashMap<String, String>>>,
}

impl CustomCommands {
    fn new() -> Self {
        CustomCommands {
            commands: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn add_command(&self, name: String, response: String) -> Result<(), String> {
        let mut commands = self.commands.lock().map_err(|_| "Failed to lock commands mutex".to_string())?;
        commands.insert(name, response);
        Ok(())
    }

    async fn get_command(&self, name: &str) -> Option<String> {
        let commands = self.commands.lock().unwrap();
        commands.get(name).cloned()
    }
}


struct Handler {
    message_log_file: Arc<Mutex<File>>,
    custom_commands: CustomCommands,
}

impl Handler {
    async fn new() -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("message_log.txt")
            .expect("Failed to open message log file");

        Handler {
            message_log_file: Arc::new(Mutex::new(file)),
            custom_commands: CustomCommands::new(),
        }
    }

    async fn log_message(&self, msg: &Message) {
        let content = format!(
            "[{}] {}: {}\n",
            msg.timestamp, msg.author.name, msg.content
        );

        let mut file = self.message_log_file.lock().unwrap();
        if let Err(e) = file.write_all(content.as_bytes()) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }

    async fn moderate_message(&self, ctx: &Context, msg: &Message) -> Result<(), Error> {
        // Example: Delete messages containing specific keywords.  Modify to suit your needs.
        let bad_words = vec!["badword1", "badword2"]; // Replace with actual bad words.

        for word in &bad_words {
            if msg.content.contains(word) {
                println!("Deleting message containing '{}'", word);
                if let Err(e) = msg.delete(ctx).await {
                    eprintln!("Failed to delete message: {}", e);
                }
                // Optionally send a warning to the user.  Be careful when using this in a selfbot
                // as you don't want to alert that it is a selfbot.
                // msg.channel_id.say(&ctx.http, "Please watch your language!").await?;
                break; // Only delete once per message.
            }
        }
        Ok(())
    }

    async fn call_external_api(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Replace with your actual API endpoint
        let api_url = "https://api.example.com/data";

        let response = reqwest::get(api_url)
            .await?
            .text()
            .await?;

        Ok(response)
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Message Logging
        self.log_message(&msg).await;

        // Moderation
        if let Err(e) = self.moderate_message(&ctx, &msg).await {
            eprintln!("Error during moderation: {}", e);
        }

        // Custom Commands
        if msg.content.starts_with("!cmd") {
             let parts: Vec<&str> = msg.content.splitn(3, ' ').collect();
             if parts.len() >= 2 {
                match parts[1] {
                    "add" => {
                        if parts.len() == 3 {
                            let command_parts: Vec<&str> = parts[2].splitn(2, ' ').collect();
                            if command_parts.len() == 2 {
                                let name = command_parts[0].to_string();
                                let response = command_parts[1].to_string();

                                if let Err(err) = self.custom_commands.add_command(name.clone(), response.clone()).await {
                                    println!("Error adding command: {}", err);
                                    if let Err(e) = msg.reply(&ctx.http, format!("Error adding command: {}", err)).await{
                                        println!("Error sending message: {}", e);
                                    }
                                } else {
                                    println!("Added command {} -> {}", name, response);
                                    if let Err(e) = msg.reply(&ctx.http, format!("Added command {} -> {}", name, response)).await{
                                        println!("Error sending message: {}", e);
                                    }
                                }


                            } else {
                                if let Err(e) = msg.reply(&ctx.http, "Usage: !cmd add <name> <response>").await{
                                    println!("Error sending message: {}", e);
                                }
                            }
                        } else {
                             if let Err(e) = msg.reply(&ctx.http, "Usage: !cmd add <name> <response>").await{
                                 println!("Error sending message: {}", e);
                             }
                        }
                    }
                    _ => {
                        if let Some(response) = self.custom_commands.get_command(parts[1]).await {
                            if let Err(e) = msg.reply(&ctx.http, response).await {
                                println!("Error sending message: {}", e);
                            }
                        } else {
                            if let Err(e) = msg.reply(&ctx.http, "Command not found.").await{
                                println!("Error sending message: {}", e);
                            }
                        }
                    }
                 }
             } else {
                if let Err(e) = msg.reply(&ctx.http, "Usage: !cmd <command>").await{
                    println!("Error sending message: {}", e);
                }
             }
        }

        // Example API Usage
        if msg.content == "!api" {
            match self.call_external_api().await {
                Ok(data) => {
                    if let Err(e) = msg.reply(&ctx.http, format!("API Data: {}", data)).await {
                        eprintln!("Error sending API response: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("API Error: {}", e);
                    if let Err(e) = msg.reply(&ctx.http, format!("API Error: {}", e)).await {
                        eprintln!("Error sending error message: {}", e);
                    }
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}


#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let handler = Handler::new().await;

    let mut client = Client::builder(&token, GatewayIntents::all()) // Use GatewayIntents::all() for self-bots
        .event_handler(handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
