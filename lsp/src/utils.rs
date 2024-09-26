use std::ops::Not;

use crate::Dotenv;

pub fn parse_dotenv(s: &str) -> Dotenv {
    let lines = s.lines();
    let dotenv = Dotenv::new();
    let mut docs = String::new();

    for line in lines {
        if line.starts_with("#") {
            docs.push_str(&line[1..].trim());
            docs.push('\n');
            continue;
        }

        if let Some((k, v)) = line.split_once("=") {
            let key = k.trim().to_owned();
            let value = {
                if let Some(commentary_pos) = v.chars().position(|c| c == '#') {
                    v[..commentary_pos].trim().to_owned()
                } else {
                    v.trim().to_owned()
                }
            };

            dotenv.insert(key, (value, docs.is_empty().not().then_some(docs.clone())));
        }
        docs = String::new();
    }

    dotenv
}
