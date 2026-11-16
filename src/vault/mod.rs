//! Password vault: one file, encrypted with XChaCha20-Poly1305 under a key
//! derived from the master passphrase with Argon2id.
//!
//! File layout: `"VSSH1" | salt (32) | nonce (24) | ciphertext+tag`. The magic
//! is authenticated as associated data. Every save uses a fresh random nonce.
//! Secrets are never logged or put into error messages.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::{fs, mem};

use anyhow::{anyhow, ensure, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Generate, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"VSSH1";
const SALT: usize = 32;
const NONCE: usize = 24;

pub struct Vault {
    path: PathBuf,
    salt: [u8; SALT],
    key: Zeroizing<[u8; 32]>,
    entries: BTreeMap<String, Zeroizing<String>>,
}

/// Argon2id, 19 MiB, 2 passes. Pinned here (not `Default`) because the file
/// does not store the parameters; changing them needs a new magic.
fn derive(passphrase: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| anyhow!("argon2: {e}"))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut *key)
        .map_err(|e| anyhow!("argon2: {e}"))?;
    Ok(key)
}

/// Length-prefixed `name, secret` pairs. A hand-rolled format so no serializer
/// keeps unzeroized copies of the secrets.
fn encode(entries: &BTreeMap<String, Zeroizing<String>>) -> Zeroizing<Vec<u8>> {
    let size = entries.iter().map(|(k, v)| 8 + k.len() + v.len()).sum();
    // Exact capacity: growing would leave unzeroized copies behind.
    let mut out = Zeroizing::new(Vec::with_capacity(size));
    for (k, v) in entries {
        for s in [k.as_bytes(), v.as_bytes()] {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s);
        }
    }
    out
}

fn take(buf: &mut &[u8]) -> Option<String> {
    let (len, rest) = buf.split_first_chunk::<4>()?;
    let (s, rest) = rest.split_at_checked(u32::from_le_bytes(*len) as usize)?;
    *buf = rest;
    String::from_utf8(s.to_vec()).ok()
}

fn decode(mut buf: &[u8]) -> Option<BTreeMap<String, Zeroizing<String>>> {
    let mut entries = BTreeMap::new();
    while !buf.is_empty() {
        let name = take(&mut buf)?;
        entries.insert(name, Zeroizing::new(take(&mut buf)?));
    }
    Some(entries)
}

impl Vault {
    /// Creates a new, empty vault file. Refuses to overwrite an existing one.
    pub fn create(path: &Path, passphrase: &str) -> Result<Self> {
        ensure!(!passphrase.is_empty(), "passphrase must not be empty");
        ensure!(!path.exists(), "vault already exists");
        let salt: [u8; SALT] = Key::generate().into();
        let vault = Self {
            path: path.to_path_buf(),
            salt,
            key: derive(passphrase, &salt)?,
            entries: BTreeMap::new(),
        };
        vault.save()?;
        Ok(vault)
    }

    pub fn open(path: &Path, passphrase: &str) -> Result<Self> {
        let data = fs::read(path).context("cannot read vault")?;
        let body = data
            .strip_prefix(MAGIC)
            .filter(|b| b.len() >= SALT + NONCE)
            .context("not a vssh vault file")?;
        let (salt, rest) = body.split_at(SALT);
        let (nonce, msg) = rest.split_at(NONCE);
        let key = derive(passphrase, salt)?;
        let nonce = XNonce::try_from(nonce).map_err(|_| anyhow!("bad nonce"))?;
        let plain = XChaCha20Poly1305::new((&*key).into())
            .decrypt(&nonce, Payload { msg, aad: MAGIC })
            .map(Zeroizing::new)
            .map_err(|_| anyhow!("wrong passphrase or damaged vault"))?;
        Ok(Self {
            path: path.to_path_buf(),
            salt: salt.try_into()?,
            key,
            entries: decode(&plain).context("vault contents are malformed")?,
        })
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|s| s.as_str())
    }

    /// Stores a secret and saves the file; an empty secret removes the entry.
    pub fn set(&mut self, name: &str, secret: &mut String) -> Result<()> {
        if secret.is_empty() {
            self.entries.remove(name);
        } else {
            self.entries
                .insert(name.to_string(), Zeroizing::new(mem::take(secret)));
        }
        self.save()
    }

    fn save(&self) -> Result<()> {
        let nonce = XNonce::generate();
        let plain = encode(&self.entries);
        let sealed = XChaCha20Poly1305::new((&*self.key).into())
            .encrypt(
                &nonce,
                Payload {
                    msg: &plain,
                    aad: MAGIC,
                },
            )
            .map_err(|_| anyhow!("encryption failed"))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        // Temp file + rename: a crash mid-write must not destroy the old vault.
        let tmp = self.path.with_extension("tmp");
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(&[MAGIC, &self.salt, &nonce, &sealed].concat())?;
        f.sync_all()?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vssh-vault-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("vault")
    }

    fn filled(path: &Path) -> Vault {
        let mut v = Vault::create(path, "correct horse").unwrap();
        v.set("web1", &mut "hunter2".to_string()).unwrap();
        v.set("db", &mut "pässwörd with spaces\n\t🔑".to_string())
            .unwrap();
        v
    }

    #[test]
    fn round_trip() {
        let path = temp("roundtrip");
        drop(filled(&path));

        let mut v = Vault::open(&path, "correct horse").unwrap();
        assert_eq!(v.get("web1"), Some("hunter2"));
        assert_eq!(v.get("db"), Some("pässwörd with spaces\n\t🔑"));
        assert_eq!(v.get("missing"), None);

        // An empty secret removes the entry, and that persists too.
        v.set("web1", &mut String::new()).unwrap();
        let v = Vault::open(&path, "correct horse").unwrap();
        assert_eq!(v.get("web1"), None);
        assert!(v.get("db").is_some());
    }

    #[test]
    fn set_takes_the_secret_out_of_the_caller() {
        let mut v = Vault::create(&temp("take"), "pw").unwrap();
        let mut secret = "s3cret".to_string();
        v.set("a", &mut secret).unwrap();
        assert!(secret.is_empty());
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let path = temp("wrongpass");
        drop(filled(&path));
        let err = Vault::open(&path, "correct horsf").err().unwrap();
        assert_eq!(err.to_string(), "wrong passphrase or damaged vault");
        assert!(Vault::open(&path, "").is_err());
    }

    #[test]
    fn tampering_is_detected() {
        let path = temp("tamper");
        drop(filled(&path));
        let good = fs::read(&path).unwrap();
        // Flip one bit in the magic, salt, nonce, ciphertext and tag in turn.
        for at in [0, 5, 40, 62, good.len() - 1] {
            let mut bad = good.clone();
            bad[at] ^= 1;
            fs::write(&path, &bad).unwrap();
            assert!(Vault::open(&path, "correct horse").is_err(), "byte {at}");
        }
        fs::write(&path, &good[..good.len() - 1]).unwrap();
        assert!(Vault::open(&path, "correct horse").is_err());
        fs::write(&path, b"VSSH1").unwrap();
        assert!(Vault::open(&path, "correct horse").is_err());
    }

    #[test]
    fn file_is_private_opaque_and_renonced() {
        let path = temp("opaque");
        let mut v = filled(&path);
        let first = fs::read(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let contains = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).any(|w| w == needle);
        assert!(!contains(&first, b"hunter2"));
        assert!(!contains(&first, b"web1"));

        // Same contents, new nonce: the ciphertext must differ entirely.
        v.set("web1", &mut "hunter2".to_string()).unwrap();
        let second = fs::read(&path).unwrap();
        assert_eq!(first.len(), second.len());
        assert_ne!(first[MAGIC.len() + SALT..], second[MAGIC.len() + SALT..]);
    }

    #[test]
    fn create_refuses_to_overwrite_or_use_empty_passphrase() {
        let path = temp("create");
        assert!(Vault::create(&path, "").is_err());
        drop(filled(&path));
        assert!(Vault::create(&path, "other").is_err());
        assert!(Vault::open(&path, "correct horse").is_ok());
    }
}
