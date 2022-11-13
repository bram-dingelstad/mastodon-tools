use vercel_lambda::{lambda, error::VercelError, Request, Response, Body, http};
use std::error::Error;
use std::collections::HashMap;

use reqwest::blocking::Client;
use reqwest::header::COOKIE;
use regex::Regex;

extern crate csv;
#[macro_use]
extern crate serde_derive;
extern crate regex;


#[derive(Debug, Deserialize)]
struct DomainBlockEntry {
    domain: String,
    public_reason: Option<String>
    // TODO: Implement other options such as private reason, etc
}

fn get_boundary(request: &Request) -> Result<&str, &'static str> {
    let boundary_parser = Regex::new(r#"multipart/form-data;\sboundary=(.*)"#)
        .expect("Syntax to be correct and not too big");

    match request.headers().get(http::header::CONTENT_TYPE) {
        Some(header_value) => {
            let value = match header_value.to_str() {
                Ok(string) => string,
                Err(_) => return Err("Couldn\'t read value of Content-Type to get multipart boundary")
            };

            match boundary_parser.captures(value) {
                Some(captures) => match captures.get(1) {
                    Some(capture) => Ok(capture.as_str()),
                    None => return Err("Couldn\'t capture value of multipart boundary")
                },
                None => return Err("Couldn\'t capture value of multipart boundary")
            }
        },
        None => return Err("Couldn\'t parse boundary for upload")
    }
}

fn handler(request: Request) -> Result<Response<String>, VercelError> {
    match mass_block(request) {
        Ok(response) => {
            Ok(
                Response::builder()
                    .status(http::StatusCode::OK)
                    .header("Content-Type", "text/html")
                    .body(format!(r#"
                        <html>
                            <head>
                                <title>Added mastodon blocks</title>
                                <meta charset="utf-8" />
                                <link rel="stylesheet" href="/sakura.css"/>
                            </head>
                            <body>

                                {}
                            </body>
                        </html>
                        "#, response))
                    .expect("Internal Server Error")
            )
        },
        Err(error) => {
            Ok(
                Response::builder()
                    .status(http::StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "text/html")
                    .body(format!(r#"
                        <html>
                            <head>
                                <title>Something went wrong</title>
                                <meta charset="utf-8" />
                                <link rel="stylesheet" href="/sakura.css"/>
                            </head>
                            <body>
                                <h1>Something went wrong</h1>
                                {}
                            </body>
                        </html>"#, error)
                    )
                    .expect("Internal Server Error")
            )
        }
    }
}

fn mass_block<'a>(request: Request) -> Result<String, &'a str> {
    let boundary = match get_boundary(&request) {
        Ok(boundary) => boundary,
        Err(message) => return Err(message)
    };

    let body: String = match request.body() {
        Body::Binary(bytes) => match std::str::from_utf8(&bytes) {
            Ok(string) => string.to_owned(),
            Err(_) => return Err("Couldn\'t deserialize body from binary")
        },
        Body::Text(text) => text.to_string(),
        Body::Empty => return Err("Received empty body, need information in order to process request")
    };

    let parts: Vec<Vec<&str>> = body.split(boundary)
        .filter_map(|entry| {
            let list: Vec<&str> = entry.trim()
                .split("\r\n\r\n")
                .filter(|string| string.len() > 0)
                .map(|string| string.strip_suffix("--").unwrap_or(string).trim())
                .collect();

            if list.len() > 1 {
                Some(list)
            } else {
                None
            }
        })
        .collect();

    let mut form_data = HashMap::new();
    let form_name_regex = Regex::new(r#"name="(.*?)""#)
        .expect("Syntax to be correct and not too big");

    // TODO: Perhaps rework this to a Struct solution
    for part in &parts {
        let name = match form_name_regex.captures(part[0]) {
            Some(captures) => match captures.get(1) {
                Some(capture) => capture.as_str(),
                None => return Err("Could not capture form name in multipart")
            },
            None => return Err("Could not capture form name in multipart")
        };

        form_data.insert(name, part[1]);
    }

    let mastodon_domain = match form_data.get("mastodon_domain") {
        Some(value) => value,
        None => return Err("No `mastodon_domain` found in your request, please try resubmitting!")
    };
    let session_id = match form_data.get("session_id") {
        Some(value) => value,
        None => return Err("No `session_id` found in your request, please try resubmitting!")
    };
    let mastodon_session_id = match form_data.get("mastodon_session_id") {
        Some(value) => value,
        None => return Err("No `session_id` found in your request, please try resubmitting!")
    };
    let blocklist_csv = match form_data.get("blocklist_csv") {
        Some(value) => value.trim(),
        None => return Err("No `blocklist_csv` found in your request, please try resubmitting!")
    };

    let client = Client::new();

    let request = client.get(format!("https://{mastodon_domain}/admin/domain_blocks/new"))
        .header(COOKIE, format!("_session_id={session_id}; _mastodon_session={mastodon_session_id}"))
        .send();

    let data = match request {
        Ok(response) => match response.text() {
            Ok(text) => text,
            Err(_) => return Err(r"Couldn't connect to your Mastodon domain for some reason, make sure your Session ID, Mastodon Session ID and Domain are filled in correctly!")
        },
        Err(_) => return Err(r"Couldn't connect to your Mastodon domain for some reason, make sure your Session ID, Mastodon Session ID and Domain are filled in correctly!")
    };

    let re = Regex::new(r#"name="authenticity_token"\svalue="(.*?)""#)
        .expect("Syntax to be correct and not too big");

    let authenticity_token = match re.captures(&data) {
        Some(captures) => match captures.get(1) {
            Some(capture) => capture.as_str(),
            None => return Err(r"Couldn't capture authenticity token from Mastodon response, check if your Mastodon Session ID, Session ID and Domain are filled in properly")
        },
        None => return Err(r"Couldn't capture authenticity token from Mastodon response, check if your Mastodon Session ID, Session ID and Domain are filled in properly")
    };

    let mut domains_to_block = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_reader(blocklist_csv.as_bytes());

    let mut result: Vec<String> = vec![];

    for row in domains_to_block.deserialize() {
        let entry: DomainBlockEntry = match row {
            Ok(entry) => entry,
            Err(error) => {
                result.push(format!("❌ Failed to parse row: {error:#?}. Make sure that your CSV is seperated by semicolons (;)."));
                continue
            }
        };

        let request = client.post(format!("https://{mastodon_domain}/admin/domain_blocks"))
            .header(COOKIE, format!("_session_id={session_id}; _mastodon_session={mastodon_session_id}"))
            .query(&[("authenticity_token", authenticity_token)])
            .query(&[("domain_block[domain]", &entry.domain)])
            .query(&[("domain_block[severity]", "suspend")])
            .query(&[("domain_block[reject_media]", "0")])
            .query(&[("domain_block[reject_reports]", "0")])
            .query(&[("domain_block[obfuscate]", "0")])
            .query(&[("domain_block[private_comment]", "")])
            .query(&[("domain_block[public_comment]", entry.public_reason)])
            .send();

        match request {
            Ok(_) => result.push(format!("✅ {} added!", &entry.domain)),
            Err(error) => result.push(format!("❌ {} failed: {error:#?}", &entry.domain))
        };
    }


    Ok(result.join("\n"))
}

// Start the runtime with the handler
#[allow(dead_code)]
fn main() -> Result<(), Box<dyn Error>> {
	Ok(lambda!(handler))
}
