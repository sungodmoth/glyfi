#![allow(unused)]

mod server_data;
mod core;
mod commands;
mod sql;
mod events;

use std::sync::Arc;
use poise::serenity_prelude as ser;
use clap::Parser;
use crate::commands::{nickname, profile, queue, update};
use crate::core::{log_command, terminate};
use crate::events::GlyfiEvents;
use crate::server_data::SERVER_ID;

/// Global context. Ugly, but this is the best way I can think
/// of to support graceful shutdown on Ctrl+C etc.
static mut __GLYFI_CONTEXT: Option<ser::Context> = None;
static mut __GLYFI_FRAMEWORK: Option<Arc<ser::ShardManager>> = None;
static mut __GLYFI_RUNTIME: Option<tokio::runtime::Handle> = None;

/// User data.
#[derive(Default)]
pub struct Data;

/// Basic types.
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
type Res = Result<(), Error>;

/// Clopts.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Whether to register the commands.
    #[clap(long, short)]
    register: bool,
}

/// Only to be called by [`terminate()`].
pub async unsafe fn __glyfi_terminate_bot() {
    if let Some(fw) = __GLYFI_FRAMEWORK.as_ref() { fw.shutdown_all().await; }
}

/// This is called from a thread that is not part of the runtime.
unsafe fn __glyfi_ctrlc_impl() {
    let handle = __GLYFI_RUNTIME.as_ref().unwrap();
    let _guard = handle.enter();
    handle.block_on(terminate());
}

/// Register bot commands.
async fn register_impl(http: impl AsRef<ser::Http>, framework: &poise::Framework<Data, Error>) -> Res {
    info_sync!("Registering commands...");
    poise::builtins::register_in_guild(
        http,
        &framework.options().commands,
        SERVER_ID,
    ).await?;
    info_sync!("Commands registered.");
    Ok(())
}

#[tokio::main]
async fn main() {
    // Register a panic hook to tear down the bot in case of an error;
    // this is so the bot restarts on error instead of hanging.
    let old_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        old_panic(info);
        std::process::abort();
    }));

    // Save runtime.
    unsafe { __GLYFI_RUNTIME = Some(tokio::runtime::Handle::current()); }

    // Register the SIGINT handler.
    //
    // Do this *after* saving the runtime as the handler will
    // attempt to enter the runtime.
    ctrlc::set_handler(|| unsafe { __glyfi_ctrlc_impl() }).expect("Failed to register SIGINT handler");

    // Initialise the database.
    unsafe { sql::__glyfi_init_db().await; }

    let args = Args::parse();
    let fw = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            pre_command: |ctx| Box::pin(async move { log_command(ctx).await; }),
            commands: vec![
                nickname(),
                profile(),
                queue(),
                update(),
            ],
            ..Default::default()
        })

        .setup(move |ctx, _, framework| {
            unsafe {
                __GLYFI_CONTEXT = Some(ctx.clone());
                __GLYFI_FRAMEWORK = Some(framework.shard_manager().clone());
            };

            Box::pin(async move {
                if args.register { register_impl(ctx, framework).await?; }
                info_sync!("Setup done");
                info_sync!("\x1b[1;33mRemember to double-check command permissions before deploying!\x1b[m");
                Ok(Default::default())
            })
        })
        .build();

    ser::ClientBuilder::new(server_data::DISCORD_BOT_TOKEN, ser::GatewayIntents::all())
        .framework(fw)
        .event_handler(GlyfiEvents)
        .await
        .unwrap()
        .start()
        .await
        .unwrap();
}
