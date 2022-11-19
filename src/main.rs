#[macro_use]
extern crate rocket;
extern crate chrono;
extern crate rexiv2;

use tokio::time::sleep;

use std::fs;

use rocket::tokio;
use structopt::StructOpt;

use chrono::prelude::DateTime;
use chrono::Local;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::{distributions::Alphanumeric, Rng};

use rocket::fs::TempFile;

use rocket::fs::FileServer;

// 직렬화 함수
use rocket::serde::{Deserialize, Serialize};

// 폼 함수
use rocket::form::{Form, FromForm};

// 플래시, 리다이렉트 함수
use rocket::response::{Flash, Redirect};

// DB 함수
use rocket_db_pools::sqlx::Row;

// DB 연결 함수
use rocket_db_pools::{
    sqlx::{self, mysql::MySqlRow},
    Connection, Database,
};

// SSR 렌더링 함수
use rocket_dyn_templates::{context, Template};

// DB 정보 정의
#[derive(Database)]
#[database("photon")]
struct Photon(sqlx::MySqlPool);

// 메인 화면
#[get("/")]
fn index() -> Template {
    Template::render("index", context! {})
}

#[derive(FromForm)]
struct Upload<'f> {
    upload: TempFile<'f>,
    second: u64,
}

#[post("/", data = "<form>")]
async fn indexpost(mut db: Connection<Photon>, mut form: Form<Upload<'_>>) -> Flash<Redirect> {
    // html 조작 방지
    if form.second > 2628000 {
        return Flash::success(Redirect::to(uri!(index)), "부적절한 요청입니다.");
    }

    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(7)
        .map(char::from)
        .collect();

    form.upload
        .persist_to(format!("./static/images/{}.png", random))
        .await
        .ok();

    let randfortokio = random.clone();

    // 메타데이터 제거하기, 컴파일러 버그때문에 스레드 분리.
    tokio::spawn(async move {
        let path = format!("./static/images/{}.png", randfortokio);

        let meta = rexiv2::Metadata::new_from_path(&path).unwrap();

        meta.clear();
        let _ = meta.save_to_file(path);
    });

    let utime = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let content: String = format!(
        r#"INSERT INTO imgtbl(imagename, view, expired, uploadtime) VALUE("{}", "{}", "{}", "{}")"#,
        random,
        0,
        utime + form.second,
        utime
    );

    match sqlx::query(&content).execute(&mut *db).await {
        Err(_e) => Flash::success(
            Redirect::to(uri!(index)),
            "업로드 중 오류가 발생하였습니다.",
        ),
        Ok(_e) => Flash::success(
            Redirect::to(uri!(image(random))),
            "성공적으로 업로드 되었습니다.",
        ),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Rowpost {
    view: i32,
    time: u64,
}

#[get("/i/<image>")]
async fn image(mut db: Connection<Photon>, image: &str) -> Template {
    let content: String = format!(
        r#"update imgtbl set view = imgtbl.view + 1 where imagename = "{}";"#,
        image
    );

    let _ = sqlx::query(&content).execute(&mut *db).await;

    let content: String = format!(
        r#"SELECT view, uploadtime FROM imgtbl WHERE imagename="{}""#,
        image
    );
    let query = sqlx::query(&content);

    let images = query.fetch_one(&mut *db).await.unwrap();

    let time: u64 = images.get("uploadtime");
    let view: i32 = images.get("view");

    let d = UNIX_EPOCH + Duration::from_secs(time);
    let datetime = DateTime::<Local>::from(d);
    let timestamp_str = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

    Template::render(
        "image",
        context! {
            time: timestamp_str,
            view: view,
            image: image
        },
    )
}

#[derive(StructOpt)]
pub struct Rowlist {
    imagename: String,
    expired: u64,
}

async fn deleteimage() {
    let pool = sqlx::MySqlPool::connect("mysql://@localhost:3306/photon")
        .await
        .unwrap();

    let content: String = "SELECT imagename, expired FROM imgtbl".to_string();
    let query = sqlx::query(&content);

    let list: Vec<Rowlist> = query
        .map(|r: MySqlRow| Rowlist {
            imagename: r.get("imagename"),
            expired: r.get("expired"),
        })
        .fetch_all(&pool)
        .await
        .unwrap();

    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for i in list {
        if i.expired <= time {
            let _ = fs::remove_file(format!("./static/images/{}.png", i.imagename));
            let content: String =
                format!(r#"DELETE FROM imgtbl WHERE imagename="{}""#, i.imagename);

            let _ = sqlx::query(&content).execute(&pool).await;
        }
    }
}

#[catch(500)]
fn internal_error() -> Template {
    Template::render("error", context! {})
}

#[catch(404)]
fn not_found() -> Template {
    Template::render("error", context! {})
}

#[launch]
fn rocket() -> _ {
    // 사진 제거 함수, 멀티 스레드로 동작.
    tokio::spawn(async move {
        loop {
            let _ = deleteimage().await;
            sleep(Duration::from_secs(5)).await;
        }
    });
    rocket::build()
        .mount("/", routes![index, indexpost, image])
        //.mount("/", FileServer::from(relative!("static")))
        .mount("/", FileServer::from("static"))
        .attach(Template::fairing())
        .attach(Photon::init())
        .register("/", catchers![internal_error, not_found])
}
