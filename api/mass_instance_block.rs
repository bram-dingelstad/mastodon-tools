use http::{StatusCode};
use vercel_lambda::{lambda, error::VercelError, IntoResponse, Request, Response, Body};
use std::error::Error;
use std::collections::HashMap;

use reqwest::blocking::Client;
use reqwest::header::COOKIE;
use regex::Regex;

use multipart_stream::parse;

extern crate csv;
#[macro_use]
extern crate serde_derive;


// TODO: Derive from serde + csv plugin
#[derive(Debug, Deserialize)]
struct DomainBlockEntry {
    domain: String,
    public_reason: Option<String>
    // TODO: Implement other options such as private reason, etc
}

// TODO: Make frontend that interacts with this endpoint
fn handler(request: Request) -> Result<impl IntoResponse, VercelError> {

    let boundary_parser = Regex::new(r#"multipart/form-data;\sboundary=(.*)"#).unwrap();

    let boundary = match request.headers().get(http::header::CONTENT_TYPE) {
        Some(value) => boundary_parser.captures(value.to_str().unwrap()).unwrap().get(1).unwrap().as_str(),
        None => return Err(VercelError::new("Couldn\'t parse boundary for upload"))
    };

    let body: String = match request.body() {
        Body::Binary(bytes) => std::str::from_utf8(&bytes).unwrap().to_owned(),
        Body::Text(text) => text.to_string(),
        Body::Empty => "".to_string()
    };

    let parts: Vec<Vec<&str>> = body.split(boundary)
        .filter_map(|entry| {
            let list: Vec<&str> = entry.trim()
                .split("\r\n")
                .filter(|string| string != &"--" && string.len() > 0)
                .collect();

            if list.len() > 1 {
                Some(list)
            } else {
                None
            }
        })
        .collect();

    let mut form_data = HashMap::new();
    let form_name_regex = Regex::new(r#"name="(.*?)""#).unwrap();

    // TODO: Perhaps rework this to a Struct solution
    for part in &parts {
        let name = form_name_regex.captures(part[0]).unwrap().get(1).unwrap().as_str();

        match part.len() {
            2 => { // Has normal content
                form_data.insert(name, part[1]);
            },
            3 => { // Has content type and buffer
                form_data.insert(name, part[2]);
            },
            _ => return Err(VercelError::new("Unexpected length of form data"))
        }
    }

    let mastodon_domain = form_data.get("mastodon_domain").unwrap();
    let session_id = form_data.get("session_id").unwrap();
    let mastodon_session_id = form_data.get("mastodon_session_id").unwrap();

    let client = Client::new();

    let data = client.get(format!("https://{mastodon_domain}/admin/domain_blocks/new"))
        .header(COOKIE, format!("_session_id={session_id}; _mastodon_session={mastodon_session_id}"))
        .send()
        .unwrap()
        .text()
        .unwrap();

    let re = Regex::new(r#"name="authenticity_token"\svalue="(.*?)""#).unwrap();

    let authenticity_token = re.captures(&data)
        .unwrap()
        .get(1)
        .unwrap()
        .as_str();

    let mut domains_to_block = csv::ReaderBuilder::new()
        .delimiter(b';')
        .from_reader(form_data.get("blocklist_csv").unwrap().trim().as_bytes());

    for row in domains_to_block.deserialize() {
        let entry: DomainBlockEntry = row.unwrap();

        client.post(format!("https://{mastodon_domain}/admin/domain_blocks"))
            .header(COOKIE, format!("_session_id={session_id}; _mastodon_session={mastodon_session_id}"))
            .query(&[("authenticity_token", authenticity_token)])
            .query(&[("domain_block[domain]", entry.domain)])
            .query(&[("domain_block[severity]", "suspend")])
            .query(&[("domain_block[reject_media]", "0")])
            .query(&[("domain_block[reject_reports]", "0")])
            .query(&[("domain_block[obfuscate]", "0")])
            .query(&[("domain_block[private_comment]", "")])
            .query(&[("domain_block[public_comment]", entry.public_reason)])
            .send()
            .unwrap();
    }

	let response = Response::builder()
		.status(StatusCode::OK)
		.header("Content-Type", "text/plain")
		.body("All instances have been blocked!")
		.expect("Internal Server Error");

		Ok(response)
}

// Start the runtime with the handler
fn main() -> Result<(), Box<dyn Error>> {
	Ok(lambda!(handler))
}
