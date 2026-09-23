use std::{env, fs};

use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use serde_json::Value;

fn main() {
    let mut args = env::args().skip(1);
    let config = args.next().expect("config path");
    let artifact = args.next().expect("artifact path");
    let signature_path = args.next().expect("signature path");
    let config: Value =
        serde_json::from_str(&fs::read_to_string(config).expect("config")).expect("config json");
    let public_key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .expect("updater public key");
    let public_key_text =
        String::from_utf8(STANDARD.decode(public_key).expect("public key base64"))
            .expect("public key utf8");
    let public_key_line = public_key_text
        .lines()
        .find(|line| !line.starts_with("untrusted comment:"))
        .expect("public key payload");
    let signature_text = String::from_utf8(
        STANDARD
            .decode(
                fs::read_to_string(signature_path)
                    .expect("signature")
                    .trim(),
            )
            .expect("signature base64"),
    )
    .expect("signature utf8");
    let signature = Signature::decode(&signature_text).expect("Tauri signature envelope");
    let key = PublicKey::from_base64(public_key_line).expect("updater public key");
    let bytes = fs::read(artifact).expect("artifact");
    key.verify(&bytes, &signature, false)
        .expect("Tauri updater signature verification");
    println!("Tauri updater signature verified");
}
