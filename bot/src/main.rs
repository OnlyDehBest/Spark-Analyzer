use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail};
use regex::Regex;
use serde_json::{Value, json};
use serenity::all::{
    Attachment, Command, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context,
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, EventHandler, GatewayIntents, GuildId, Interaction, Message,
    Ready, UserId,
};
use serenity::async_trait;
use sparkscope::i18n::{self, Lang};
use sparkscope::{discord, loader, pipeline};
use tokio::sync::{Mutex, Semaphore};

const MAX_FILE: u32 = 50 * 1024 * 1024;
const GUILD_COOLDOWN: Duration = Duration::from_secs(10);

struct Bot {
    site: String,
    langs: Mutex<HashMap<u64, Lang>>,
    langs_file: PathBuf,
    cooldown: Mutex<HashMap<u64, Instant>>,
    slots: Semaphore,
    link_re: Regex,
}

fn t(lang: Lang, it: &'static str, en: &'static str) -> &'static str {
    if lang == Lang::En { en } else { it }
}

impl Bot {
    async fn lang(&self, user: UserId, client_locale: Option<&str>) -> Lang {
        if let Some(l) = self.langs.lock().await.get(&user.get()) {
            return *l;
        }
        match client_locale {
            Some(l) if !l.is_empty() && !l.starts_with("it") => Lang::En,
            _ => Lang::default(),
        }
    }

    async fn set_lang(&self, user: UserId, lang: Lang) -> Result<()> {
        let mut map = self.langs.lock().await;
        map.insert(user.get(), lang);
        let data: HashMap<String, &str> =
            map.iter().map(|(k, v)| (k.to_string(), v.code())).collect();
        let body = serde_json::to_string_pretty(&data)?;
        drop(map);
        if let Some(dir) = self.langs_file.parent() {
            tokio::fs::create_dir_all(dir).await.ok();
        }
        let tmp = self.langs_file.with_extension("json.tmp");
        tokio::fs::write(&tmp, body).await?;
        tokio::fs::rename(&tmp, &self.langs_file).await?;
        Ok(())
    }

    async fn allowed(&self, guild: Option<GuildId>) -> bool {
        let Some(g) = guild else { return true };
        let mut map = self.cooldown.lock().await;
        let now = Instant::now();
        match map.get(&g.get()) {
            Some(last) if now.duration_since(*last) < GUILD_COOLDOWN => false,
            _ => {
                map.insert(g.get(), now);
                true
            }
        }
    }

    async fn analyze(&self, source: Source, lang: Lang) -> Result<Value> {
        let _slot = self.slots.acquire().await?;
        let site = self.site.clone();
        tokio::task::spawn_blocking(move || -> Result<Value> {
            let _g = i18n::scoped(lang);
            let loaded = match source {
                Source::Link(code) => loader::load(&code, loader::DEFAULT_BYTEBIN)?,
                Source::File { url, name } => {
                    let bytes = download(&url)?;
                    loader::load_bytes(&bytes, &name)?
                }
            };
            let (report, _, _) = pipeline::build(loaded, None)?;
            Ok(discord::message(&report, &site))
        })
        .await
        .map_err(|e| anyhow!("analisi interrotta: {e}"))?
    }
}

enum Source {
    Link(String),
    File { url: String, name: String },
}

fn download(url: &str) -> Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(concat!(
            "spark-resource-analyzer-bot/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .into();
    let mut resp = agent
        .get(url)
        .call()
        .context("download dell'allegato fallito")?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_FILE as u64)
        .read_to_vec()
        .context("allegato troppo grande o non leggibile")?;
    Ok(bytes)
}

fn error_text(lang: Lang, e: &anyhow::Error) -> String {
    let msg: String = format!("{e:#}").chars().take(300).collect();
    format!(
        "{} {msg}",
        t(
            lang,
            "⚠️ Non riesco ad analizzare il profilo:",
            "⚠️ I can't analyze the profile:"
        )
    )
}

fn commands() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("analyze")
            .name_localized("it", "analizza")
            .description("Analyze a spark profile: link or .sparkprofile file")
            .description_localized("it", "Analizza un profilo spark: link o file .sparkprofile")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "link",
                    "spark.lucko.me link or just the code",
                )
                .description_localized("it", "Link spark.lucko.me o solo il codice")
                .required(false),
            )
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::Attachment,
                    "file",
                    ".sparkprofile, .sparkheap or .sparkhealth file",
                )
                .description_localized("it", "File .sparkprofile, .sparkheap o .sparkhealth")
                .required(false),
            ),
        CreateCommand::new("language")
            .name_localized("it", "lingua")
            .description("Choose the language the bot uses with you")
            .description_localized("it", "Scegli la lingua in cui il bot ti risponde")
            .dm_permission(true)
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "language", "Language")
                    .name_localized("it", "lingua")
                    .description_localized("it", "Lingua")
                    .required(true)
                    .add_string_choice_localized("Italiano", "it", [("it", "Italiano")])
                    .add_string_choice_localized("English", "en", [("it", "Inglese")]),
            ),
    ]
}

async fn reply_ephemeral(ctx: &Context, cmd: &CommandInteraction, text: String) {
    let r = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(text)
            .ephemeral(true),
    );
    if let Err(e) = cmd.create_response(&ctx.http, r).await {
        eprintln!("risposta non inviata: {e}");
    }
}

impl Bot {
    async fn on_language(&self, ctx: &Context, cmd: &CommandInteraction) {
        let code = cmd
            .data
            .options
            .iter()
            .find_map(|o| match &o.value {
                CommandDataOptionValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let lang = Lang::parse(&code);
        let text = match self.set_lang(cmd.user.id, lang).await {
            Ok(()) => t(
                lang,
                "✅ Da ora il bot ti risponde in **italiano**. Vale solo per te, in tutti i server.",
                "✅ From now on the bot replies to you in **English**. This only applies to you, in every server.",
            )
            .to_string(),
            Err(e) => {
                eprintln!("salvataggio lingua fallito: {e:#}");
                t(
                    lang,
                    "⚠️ Lingua impostata, ma non sono riuscito a salvarla: tornerà quella predefinita al riavvio.",
                    "⚠️ Language set, but I couldn't save it: it will go back to the default after a restart.",
                )
                .to_string()
            }
        };
        reply_ephemeral(ctx, cmd, text).await;
    }

    async fn on_analyze(&self, ctx: &Context, cmd: &CommandInteraction) {
        let lang = self.lang(cmd.user.id, Some(cmd.locale.as_str())).await;
        let mut link: Option<String> = None;
        let mut file: Option<&Attachment> = None;
        for o in &cmd.data.options {
            match &o.value {
                CommandDataOptionValue::String(s) => link = Some(s.trim().to_string()),
                CommandDataOptionValue::Attachment(id) => {
                    file = cmd.data.resolved.attachments.get(id)
                }
                _ => {}
            }
        }
        let source =
            match (file, link) {
                (Some(a), _) if a.size > MAX_FILE => {
                    reply_ephemeral(
                        ctx,
                        cmd,
                        t(
                            lang,
                            "⚠️ File troppo grande (massimo 50 MB).",
                            "⚠️ File too large (50 MB max).",
                        )
                        .into(),
                    )
                    .await;
                    return;
                }
                (Some(a), _) => Source::File {
                    url: a.url.clone(),
                    name: a.filename.clone(),
                },
                (None, Some(l)) => match self
                    .link_re
                    .captures(&l)
                    .map(|c| c[1].to_string())
                    .or_else(|| {
                        (l.len() >= 6 && l.chars().all(|c| c.is_ascii_alphanumeric()))
                            .then(|| l.clone())
                    }) {
                    Some(code) => Source::Link(code),
                    None => {
                        reply_ephemeral(
                            ctx,
                            cmd,
                            t(
                                lang,
                                "⚠️ Link non valido. Esempio: https://spark.lucko.me/AbCd1234",
                                "⚠️ Invalid link. Example: https://spark.lucko.me/AbCd1234",
                            )
                            .into(),
                        )
                        .await;
                        return;
                    }
                },
                (None, None) => {
                    reply_ephemeral(
                        ctx,
                        cmd,
                        t(
                            lang,
                            "Indica un link spark oppure allega un file .sparkprofile.",
                            "Give a spark link or attach a .sparkprofile file.",
                        )
                        .into(),
                    )
                    .await;
                    return;
                }
            };
        if !self.allowed(cmd.guild_id).await {
            reply_ephemeral(
                ctx,
                cmd,
                t(
                    lang,
                    "⏳ Un'analisi alla volta: riprova tra qualche secondo.",
                    "⏳ One analysis at a time: try again in a few seconds.",
                )
                .into(),
            )
            .await;
            return;
        }
        if let Err(e) = cmd.defer(&ctx.http).await {
            eprintln!("defer fallito: {e}");
            return;
        }
        let body = match self.analyze(source, lang).await {
            Ok(m) => m,
            Err(e) => json!({ "content": error_text(lang, &e) }),
        };
        if let Err(e) = ctx
            .http
            .edit_original_interaction_response(&cmd.token, &body, vec![])
            .await
        {
            eprintln!("risposta all'analisi non inviata: {e}");
        }
    }
}

#[async_trait]
impl EventHandler for Bot {
    async fn ready(&self, ctx: Context, ready: Ready) {
        match Command::set_global_commands(&ctx.http, commands()).await {
            Ok(c) => println!(
                "{} connesso: {} comandi registrati, {} server",
                ready.user.name,
                c.len(),
                ready.guilds.len()
            ),
            Err(e) => eprintln!("registrazione comandi fallita: {e}"),
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        let Some(code) = self
            .link_re
            .captures(&msg.content)
            .map(|c| c[1].to_string())
        else {
            return;
        };
        if !self.allowed(msg.guild_id).await {
            return;
        }
        let lang = self.lang(msg.author.id, None).await;
        let typing = msg.channel_id.start_typing(&ctx.http);
        let mut body = match self.analyze(Source::Link(code), lang).await {
            Ok(m) => m,
            Err(e) => {
                json!({ "content": error_text(lang, &e), "allowed_mentions": { "parse": [] } })
            }
        };
        typing.stop();
        body["message_reference"] =
            json!({ "message_id": msg.id.get().to_string(), "fail_if_not_exists": false });
        if let Err(e) = ctx.http.send_message(msg.channel_id, vec![], &body).await {
            eprintln!("risposta al link non inviata: {e}");
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Interaction::Command(cmd) = interaction else {
            return;
        };
        match cmd.data.name.as_str() {
            "language" => self.on_language(&ctx, &cmd).await,
            "analyze" => self.on_analyze(&ctx, &cmd).await,
            _ => {}
        }
    }
}

fn load_langs(path: &PathBuf) -> HashMap<u64, Lang> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&text) else {
        eprintln!("{} non valido: ignorato", path.display());
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, Lang::parse(&v))))
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = std::env::var("DISCORD_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .context("manca DISCORD_TOKEN (il token del bot dal Discord Developer Portal)")?;
    let site = std::env::var("SITE_URL").unwrap_or_else(|_| discord::DEFAULT_SITE.to_string());
    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into()));
    let parallel: usize = std::env::var("MAX_PARALLEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v > 0)
        .unwrap_or(2);
    let langs_file = data_dir.join("lingue-utenti.json");
    let bot = Arc::new(Bot {
        site,
        langs: Mutex::new(load_langs(&langs_file)),
        langs_file,
        cooldown: Mutex::new(HashMap::new()),
        slots: Semaphore::new(parallel),
        link_re: Regex::new(r"(?i)spark\.lucko\.me/([A-Za-z0-9]{6,})")?,
    });
    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let mut client = serenity::Client::builder(&token, intents)
        .event_handler_arc(bot)
        .await
        .context("creazione del client Discord fallita")?;
    let shards = client.shard_manager.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("arresto…");
        shards.shutdown_all().await;
    });
    if let Err(e) = client.start().await {
        bail!("connessione a Discord fallita: {e} (token valido? intent Message Content attivo?)");
    }
    Ok(())
}
