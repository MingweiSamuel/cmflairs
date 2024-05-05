#![warn(missing_docs)]

//! Cloudflare worker.

use futures::future::join_all;
use ignore_keys::IgnoreKeys;
use riven::consts::{Champion, PlatformRoute, RegionalRoute};
use serde_with::de::DeserializeAsWrap;
use serde_with::{DeserializeAs, Same};
use util::get_rgapi;
use worker::{
    event, query, Context, D1Argument, D1Database, D1PreparedStatement, Env, Error, MessageBatch,
    MessageExt, Request, Response, Result,
};

pub mod db;
pub mod ignore_keys;
pub mod util;
pub mod webjob;

/// Local region.
pub const ROUTE: RegionalRoute = RegionalRoute::AMERICAS;

/// Cloudflare queue handler.
#[event(queue)]
pub async fn queue(
    message_batch: MessageBatch<webjob::Task>,
    env: Env,
    _ctx: Context,
) -> Result<()> {
    util::init_logging();

    let rgapi = get_rgapi(&env);

    let futures = message_batch.messages()?.into_iter().map(|msg| {
        log::info!("Handling webjob task: {:?}", msg.body());
        let db = env.d1("BINDING_D1_DB").unwrap();
        webjob::handle(db, rgapi, msg)
    });
    let results = join_all(futures).await;
    let errors = results
        .into_iter()
        .filter_map(|result| result.map(|msg| msg.ack()).err())
        .collect::<Vec<_>>();

    errors
        .is_empty()
        .then_some(())
        .ok_or(Error::RustError(format!("{:?}", errors)))
}

/// Cloudflare fetch request handler.
#[event(fetch, respond_with_errors)]
pub async fn fetch(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    util::init_logging();

    let queue = env.queue("BINDING_QUEUE_WEBJOB").unwrap();
    queue.send(webjob::Task::UpdateSummoner(1)).await?;

    let db = env.d1("BINDING_D1_DB").unwrap();

    let query = query!(&db, "SELECT * FROM user");
    let mut response1: Vec<db::User> = query.all().await?.results()?;

    for user in response1.iter_mut() {
        let query = query!(&db, "SELECT * FROM summoner WHERE user_id = ?1", user.id)?;
        user.summoners = Some(query.all().await?.results()?);
    }

    let query = query!(&db, "SELECT id, platform FROM summoner WHERE id = 1");

    type X =
        DeserializeAsWrap<(u64, PlatformRoute), IgnoreKeys<(Same, serde_with::DisplayFromStr)>>;
    let (pk, platform) = query
        .first::<X>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("Failed to find summoner with PK ID: 1")))?
        .into_inner();

    Response::ok(format!("{:#?}\n\n{:#?}", response1, (pk, platform)))
}

// pub trait Table {
//     /// The name of the table.
//     const TABLE: &'static str;
//     /// The name of the key field.
//     const KEY: &'static str;
//     /// The name of all non-key fields.
//     const FIELDS: &'static [&'static str];

//     fn key(&self) -> wasm_bindgen::JsValue;

//     fn fields(&self, fields: &[&str]) -> Vec<wasm_bindgen::JsValue>;

//     fn update(&self, db: &D1Database, fields: &[&str]) -> D1PreparedStatement {
//         let mut fields_and_key = self.fields(fields);
//         fields_and_key.push(self.key());
//         update_query(db, Self::TABLE, fields, Self::KEY)
//             .bind(&*fields_and_key)
//             .unwrap()
//     }
//     // fn update_batch(items: &[&Self], db: &D1Database, fields: &[&str]) -> Vec<D1PreparedStatement> {
//     //     let prepared_statement = update_query(db, Self::TABLE, fields, Self::KEY);
//     //     items
//     //         .iter()
//     //         .map(|this| {
//     //             let mut fields_and_key = this.fields(fields);
//     //             fields_and_key.push(this.key());
//     //             prepared_statement.bind(&*fields_and_key).unwrap()
//     //         })
//     //         .collect()
//     // }
// }

// fn update_query(db: &D1Database, table: &str, fields: &[&str], key: &str) -> D1PreparedStatement {
//     assert_ne!(0, fields.len());

//     use std::fmt::Write;

//     let mut query = String::new();
//     writeln!(&mut query, "UPDATE {} SET", table).unwrap();

//     let mut iter = fields.iter().peekable();
//     while let Some(&field) = iter.next() {
//         write!(&mut query, "    {} = ?", field).unwrap();
//         if iter.peek().is_some() {
//             writeln!(&mut query, ",").unwrap();
//         }
//     }
//     writeln!(&mut query, "WHERE {} = ?", key).unwrap();

//     db.prepare(query)
// }
