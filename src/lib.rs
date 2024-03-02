use riven::consts::PlatformRoute;
use worker::{event, Context, Date, Env, Request, Response, Result};

mod util;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    util::init_logging();

    log::info!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or("unknown region".into())
    );

    let riot_api = util::get_rgapi(&env);

    let summoner = riot_api
        .summoner_v4()
        .get_by_summoner_name(PlatformRoute::NA1, "LugnutsK")
        .await
        .unwrap()
        .unwrap();
    Response::ok(format!("Hello {}!", &summoner.name))

    // // Optionally, use the Router to handle matching endpoints, use ":name" placeholders, or "*name"
    // // catch-alls to match on specific patterns. Alternatively, use `Router::with_data(D)` to
    // // provide arbitrary data that will be accessible in each route via the `ctx.data()` method.
    // let router = Router::new();

    // // Add as many routes as your Worker needs! Each route will get a `Request` for handling HTTP
    // // functionality and a `RouteContext` which you can use to  and get route parameters and
    // // Environment bindings like KV Stores, Durable Objects, Secrets, and Variables.
    // router
    //     .get_async("/", |_, context| async {
    //         let riot_api = RiotApi::new("");
    //         let summoner = riot_api.summoner_v4().get_by_summoner_name(PlatformRoute::NA1, "LugnutsK").await.unwrap().unwrap();
    //         Response::ok(&format!("Hello {}!", "hello")) //&summoner.name))
    //     })
    //     .post_async("/form/:field", |mut req, ctx| async move {
    //         if let Some(name) = ctx.param("field") {
    //             let form = req.form_data().await?;
    //             match form.get(name) {
    //                 Some(FormEntry::Field(value)) => {
    //                     return Response::from_json(&json!({ name: value }))
    //                 }
    //                 Some(FormEntry::File(_)) => {
    //                     return Response::error("`field` param in form shouldn't be a File", 422);
    //                 }
    //                 None => return Response::error("Bad Request", 400),
    //             }
    //         }

    //         Response::error("Bad Request", 400)
    //     })
    //     .get("/worker-version", |_, ctx| {
    //         let version = ctx.var("WORKERS_RS_VERSION")?.to_string();
    //         Response::ok(version)
    //     })
    //     .run(req, env)
    //     .await
}
