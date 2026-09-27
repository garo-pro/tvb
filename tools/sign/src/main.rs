//! Non-interactive minisign helper for TV-Blind releases.
//!
//! The usual minisign tools read the key password from the console, which CI
//! doesn't have. This one takes everything from the environment:
//!
//! - `TVB_SIGNING_PASSWORD`: password of the secret key (both commands).
//! - `TVB_SIGNING_KEY`: the secret key file's contents (`sign` only).
//!
//! Usage:
//!
//! ```text
//! tvb-sign keygen <dir>   writes <dir>/tvb.key and <dir>/tvb.pub, prints the public key
//! tvb-sign sign <file>    writes <file>.minisig and verifies it
//! ```

use std::env;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::Path;
use std::process::ExitCode;

use minisign::{KeyPair, PublicKey, SecretKeyBox, SignatureBox};

const PASSWORD_VAR: &str = "TVB_SIGNING_PASSWORD";
const KEY_VAR: &str = "TVB_SIGNING_KEY";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen", dir] => keygen(Path::new(dir)),
        ["sign", file] => sign(Path::new(file)),
        _ => Err("usage: tvb-sign keygen <dir> | tvb-sign sign <file>".to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr(), "tvb-sign: {e}");
            ExitCode::FAILURE
        }
    }
}

fn password() -> Result<String, String> {
    env::var(PASSWORD_VAR).ok().filter(|p| !p.is_empty()).ok_or_else(|| format!("{PASSWORD_VAR} is not set"))
}

fn keygen(dir: &Path) -> Result<(), String> {
    let key_path = dir.join("tvb.key");
    if key_path.exists() {
        return Err(format!("{} already exists; refusing to overwrite a signing key", key_path.display()));
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let pk_file = File::create(dir.join("tvb.pub")).map_err(|e| e.to_string())?;
    let sk_file = File::create_new(&key_path).map_err(|e| e.to_string())?;
    let pair = KeyPair::generate_and_write_encrypted_keypair(
        pk_file,
        sk_file,
        Some("TV-Blind release key"),
        Some(password()?),
    )
    .map_err(|e| e.to_string())?;
    // The public key is meant to be shared; it goes into the app.
    let _ = writeln!(io::stdout(), "{}", pair.pk.to_base64());
    Ok(())
}

fn sign(file: &Path) -> Result<(), String> {
    let key_text = env::var(KEY_VAR).map_err(|_| format!("{KEY_VAR} is not set"))?;
    let password = password()?;
    let sk = SecretKeyBox::from_string(key_text.trim())
        .and_then(|b| b.into_secret_key(Some(password)))
        .map_err(|e| format!("could not unlock the secret key: {e}"))?;
    let pk = PublicKey::from_secret_key(&sk).map_err(|e| e.to_string())?;

    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let trusted = format!("file:{name}");
    let data = File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let sig = minisign::sign(Some(&pk), &sk, data, Some(&trusted), Some("TV-Blind release signature"))
        .map_err(|e| e.to_string())?;

    // Verify before writing, so a broken signature never gets published.
    let sig_box = SignatureBox::from_string(&sig.to_string()).map_err(|e| e.to_string())?;
    let data = File::open(file).map_err(|e| e.to_string())?;
    minisign::verify(&pk, &sig_box, data, true, false, false).map_err(|e| format!("verification failed: {e}"))?;

    let sig_path = format!("{}.minisig", file.display());
    fs::write(&sig_path, sig.to_string()).map_err(|e| e.to_string())?;
    let _ = writeln!(io::stdout(), "signed {name} with key {}", pk.to_base64());
    Ok(())
}
