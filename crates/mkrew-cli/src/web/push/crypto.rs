//! Web Push message encryption (RFC 8291, `aes128gcm` of RFC 8188) and the VAPID header
//! (RFC 8292), on `ring` (ADR 0066). `seal` is the deterministic core the RFC vector tests;
//! `encrypt` adds a fresh sender key and salt.

use anyhow::{Context, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey};
use ring::hkdf::{self, HKDF_SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};

/// One record holds the whole message; the header says 4096 like the RFC example.
const RECORD_SIZE: u32 = 4096;
/// 4096 − 86 (header) − 1 (padding delimiter) − 16 (tag): the most a push service must accept
/// (RFC 8291 §4).
pub const MAX_PLAINTEXT: usize = 3993;
const P256_POINT: usize = 65;
const AUTH_SECRET: usize = 16;
/// VAPID tokens live 12 h (RFC 8292 allows at most 24 h).
const JWT_LIFETIME: u64 = 12 * 3600;
/// The VAPID `sub` claim. Apple's push service answers 403 `BadJwtToken` to `…@localhost`;
/// a contact on a dotted domain is what its error reports ask for.
const SUBJECT: &str = "mailto:mkrew@example.com";

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand<const N: usize>(prk: &hkdf::Prk, info: &[&[u8]]) -> anyhow::Result<[u8; N]> {
    let mut out = [0u8; N];
    prk.expand(info, Len(N))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| anyhow!("hkdf expand"))?;
    Ok(out)
}

/// RFC 8291 §3.3: the input keying material from the ECDH secret and the auth secret.
fn input_key(
    ecdh_secret: &[u8],
    auth: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
) -> anyhow::Result<[u8; 32]> {
    let prk = hkdf::Salt::new(HKDF_SHA256, auth).extract(ecdh_secret);
    expand::<32>(&prk, &[b"WebPush: info\0", ua_public, as_public])
}

/// RFC 8291 §3.4 / RFC 8188 §2.2: the content encryption key and nonce.
fn content_keys(ikm: &[u8; 32], salt: &[u8]) -> anyhow::Result<([u8; 16], [u8; 12])> {
    let prk = hkdf::Salt::new(HKDF_SHA256, salt).extract(ikm);
    Ok((
        expand::<16>(&prk, &[b"Content-Encoding: aes128gcm\0"])?,
        expand::<12>(&prk, &[b"Content-Encoding: nonce\0"])?,
    ))
}

/// The request body for one phone given the ECDH secret, sender key and salt: header
/// (salt, record size, key id = sender key) then the single AES-128-GCM record.
pub fn seal(
    ecdh_secret: &[u8],
    auth: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
    salt: &[u8; 16],
    plaintext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if plaintext.len() > MAX_PLAINTEXT {
        bail!(
            "push message of {} bytes is over {MAX_PLAINTEXT}",
            plaintext.len()
        );
    }
    if ua_public.len() != P256_POINT || as_public.len() != P256_POINT || auth.len() != AUTH_SECRET {
        bail!("push keys have the wrong size");
    }
    let ikm = input_key(ecdh_secret, auth, ua_public, as_public)?;
    let (cek, nonce) = content_keys(&ikm, salt)?;
    let mut record = Vec::with_capacity(plaintext.len() + 1 + 16);
    record.extend_from_slice(plaintext);
    record.push(0x02); // the last (and only) record's padding delimiter
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_128_GCM, &cek).map_err(|_| anyhow!("aes key"))?,
    );
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::empty(),
        &mut record,
    )
    .map_err(|_| anyhow!("aes-gcm seal"))?;
    let mut body = Vec::with_capacity(86 + record.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(65);
    body.extend_from_slice(as_public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// Encrypts `plaintext` for the phone with public key `ua_public` (65-byte uncompressed P-256
/// point) and auth secret `auth` (16 bytes), both from its push subscription.
pub fn encrypt(ua_public: &[u8], auth: &[u8], plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let private = EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| anyhow!("no randomness"))?;
    let as_public = private
        .compute_public_key()
        .map_err(|_| anyhow!("sender public key"))?;
    // `ring` checks that the phone's key is a point on the curve.
    let secret = agreement::agree_ephemeral(
        private,
        &UnparsedPublicKey::new(&agreement::ECDH_P256, ua_public),
        |secret| secret.to_vec(),
    )
    .map_err(|_| anyhow!("the phone's key is not a valid P-256 point"))?;
    let mut salt = [0u8; 16];
    rng.fill(&mut salt).map_err(|_| anyhow!("no randomness"))?;
    seal(
        &secret,
        auth,
        ua_public,
        as_public.as_ref(),
        &salt,
        plaintext,
    )
}

/// `https://host[:port]` of `url`: the VAPID `aud`. `None` unless it is an `https` URL.
pub fn origin(url: &str) -> Option<&str> {
    const SCHEME: &str = "https://";
    let rest = url.strip_prefix(SCHEME)?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&url[..SCHEME.len() + end])
}

/// The application server's VAPID key pair (P-256 ECDSA).
pub struct Vapid {
    pair: EcdsaKeyPair,
    pkcs8: Vec<u8>,
}

impl Vapid {
    pub fn generate() -> anyhow::Result<Self> {
        let document =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .map_err(|_| anyhow!("no randomness"))?;
        Self::from_pkcs8(document.as_ref())
    }

    pub fn from_pkcs8(pkcs8: &[u8]) -> anyhow::Result<Self> {
        let pair = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8,
            &SystemRandom::new(),
        )
        .map_err(|e| anyhow!("VAPID key: {e}"))?;
        Ok(Self {
            pair,
            pkcs8: pkcs8.to_vec(),
        })
    }

    /// The key as stored (`<config>/web_push.json`).
    pub fn pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    /// The 65-byte uncompressed public key: the page's `applicationServerKey`.
    pub fn public_key(&self) -> &[u8] {
        self.pair.public_key().as_ref()
    }

    /// The `Authorization` header for a POST to `endpoint` at unix time `now`.
    pub fn authorization(&self, endpoint: &str, now: u64) -> anyhow::Result<String> {
        let aud = origin(endpoint).context("push endpoint is not an https URL")?;
        let claims = serde_json::json!({ "aud": aud, "exp": now + JWT_LIFETIME, "sub": SUBJECT });
        let signed = format!(
            "{}.{}",
            B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#),
            B64.encode(claims.to_string())
        );
        let signature = self
            .pair
            .sign(&SystemRandom::new(), signed.as_bytes())
            .map_err(|_| anyhow!("VAPID signature"))?;
        Ok(format!(
            "vapid t={signed}.{}, k={}",
            B64.encode(signature.as_ref()),
            B64.encode(self.public_key())
        ))
    }
}

#[cfg(test)]
mod tests {
    use ring::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey as SigPublicKey};

    use super::*;

    fn b(text: &str) -> Vec<u8> {
        B64.decode(text).unwrap()
    }

    // RFC 8291 Appendix A (and §5): every value below is copied from the RFC.
    const PLAINTEXT: &str = "V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24";
    const AS_PUBLIC: &str =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    const ECDH_SECRET: &str = "kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs";
    const IKM: &str = "S4lYMb_L0FxCeq0WhDx813KgSYqU26kOyzWUdsXYyrg";
    const CEK: &str = "oIhVW04MRdy2XN9CiKLxTg";
    const NONCE: &str = "4h_95klXJ5E_qnoN";
    const HEADER: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const CIPHERTEXT: &str =
        "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ";
    /// The whole body of RFC 8291 §5 (the three base64url lines joined).
    const BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";

    #[test]
    fn the_rfc_8291_keys_come_out_as_in_the_appendix() {
        let ikm = input_key(&b(ECDH_SECRET), &b(AUTH), &b(UA_PUBLIC), &b(AS_PUBLIC)).unwrap();
        assert_eq!(ikm.to_vec(), b(IKM));
        let (cek, nonce) = content_keys(&ikm, &b(SALT)).unwrap();
        assert_eq!(cek.to_vec(), b(CEK));
        assert_eq!(nonce.to_vec(), b(NONCE));
    }

    #[test]
    fn the_rfc_8291_message_is_reproduced_byte_for_byte() {
        let salt: [u8; 16] = b(SALT).try_into().unwrap();
        let body = seal(
            &b(ECDH_SECRET),
            &b(AUTH),
            &b(UA_PUBLIC),
            &b(AS_PUBLIC),
            &salt,
            &b(PLAINTEXT),
        )
        .unwrap();
        // The appendix gives the 86-byte header and the ciphertext separately ...
        let mut expected = b(HEADER);
        expected.extend(b(CIPHERTEXT));
        assert_eq!(body, expected);
        // ... and §5 gives the same bytes as one body (144 octets: 86 + 41 + 1 + 16).
        assert_eq!(body, b(BODY));
        assert_eq!(body.len(), 144);
    }

    /// The phone's side of RFC 8291, with `ring`: proves `encrypt`'s ECDH half.
    fn decrypt(
        ua_private: EphemeralPrivateKey,
        ua_public: &[u8],
        auth: &[u8],
        body: &[u8],
    ) -> Vec<u8> {
        let (salt, rest) = body.split_at(16);
        assert_eq!(&rest[..4], &RECORD_SIZE.to_be_bytes());
        assert_eq!(rest[4], 65, "keyid is the 65-byte sender key");
        let as_public = &rest[5..70];
        let secret = agreement::agree_ephemeral(
            ua_private,
            &UnparsedPublicKey::new(&agreement::ECDH_P256, as_public),
            |s| s.to_vec(),
        )
        .unwrap();
        let ikm = input_key(&secret, auth, ua_public, as_public).unwrap();
        let (cek, nonce) = content_keys(&ikm, salt).unwrap();
        let key = LessSafeKey::new(UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
        let mut record = rest[70..].to_vec();
        let plain = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::empty(),
                &mut record,
            )
            .unwrap();
        assert_eq!(plain.last(), Some(&2), "padding delimiter");
        plain[..plain.len() - 1].to_vec()
    }

    #[test]
    fn a_phone_can_read_what_encrypt_made_and_nobody_else() {
        let rng = SystemRandom::new();
        let ua_private = EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_public = ua_private.compute_public_key().unwrap().as_ref().to_vec();
        let auth = [7u8; 16];
        let text = r#"{"title":"omp · mkrew","body":"needs you","session":"01J0"}"#.as_bytes();
        let body = encrypt(&ua_public, &auth, text).unwrap();
        assert_eq!(body.len(), 86 + text.len() + 1 + 16);
        assert_eq!(decrypt(ua_private, &ua_public, &auth, &body), text);
        // A second message differs (fresh sender key and salt every time).
        assert_ne!(body, encrypt(&ua_public, &auth, text).unwrap());
    }

    #[test]
    fn bad_phone_keys_and_oversize_payloads_are_refused() {
        let ok_key = b(UA_PUBLIC);
        let auth = b(AUTH);
        assert!(encrypt(&ok_key, &auth, b"{}").is_ok());
        assert!(
            encrypt(&[4u8; 65], &auth, b"{}").is_err(),
            "not a point on P-256"
        );
        assert!(encrypt(&ok_key[..64], &auth, b"{}").is_err(), "short key");
        assert!(
            encrypt(&ok_key, &auth[..15], b"{}").is_err(),
            "short auth secret"
        );
        assert!(encrypt(&ok_key, &auth, "x".repeat(MAX_PLAINTEXT).as_bytes()).is_ok());
        assert!(encrypt(&ok_key, &auth, "x".repeat(MAX_PLAINTEXT + 1).as_bytes()).is_err());
    }

    /// RFC 8292 §2.4: a real token and the key that signed it.
    #[test]
    fn the_rfc_8292_example_token_verifies_with_our_checker() {
        // The three dot-separated parts of the RFC's `t=` value (header, claims, signature).
        let jwt = format!(
            "{}.{}.{}",
            "eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9",
            "eyJhdWQiOiJodHRwczovL3B1c2guZXhhbXBsZS5uZXQiLCJleHAiOjE0NTM1MjM3NjgsInN1YiI6Im1haWx0bzpwdXNoQGV4YW1wbGUuY29tIn0",
            "i3CYb7t4xfxCDquptFOepC9GAu_HLGkMlMuCGSK2rpiUfnK9ojFwDXb1JrErtmysazNjjvW2L9OkSSHzvoD1oA",
        );
        let key = b(
            "BA1Hxzyi1RUM1b5wjxsn7nGxAszw2u61m164i3MrAIxHF6YK5h4SDYic-dRuU_RCPCfA5aq9ojSwk5Y2EmClBPs",
        );
        assert!(verify_jwt(&jwt, &key));
        let tampered = jwt.replacen("eyJhdWQ", "eyJhdWR", 1);
        assert!(!verify_jwt(&tampered, &key));
    }

    fn verify_jwt(jwt: &str, public_key: &[u8]) -> bool {
        let (signed, signature) = jwt.rsplit_once('.').unwrap();
        SigPublicKey::new(&ECDSA_P256_SHA256_FIXED, public_key)
            .verify(signed.as_bytes(), &b(signature))
            .is_ok()
    }

    #[test]
    fn our_vapid_header_has_the_rfc_8292_shape() {
        let vapid = Vapid::generate().unwrap();
        let now = 1_700_000_000;
        let header = vapid
            .authorization("https://web.push.apple.com/abc123?x=1", now)
            .unwrap();
        let rest = header.strip_prefix("vapid t=").expect("vapid scheme");
        let (jwt, key) = rest.split_once(", k=").expect("t and k parameters");
        assert_eq!(b(key), vapid.public_key(), "k is the public key");
        assert_eq!(vapid.public_key().len(), 65);
        assert!(verify_jwt(jwt, vapid.public_key()), "signature");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(b(parts[0]), br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims: serde_json::Value = serde_json::from_slice(&b(parts[1])).unwrap();
        assert_eq!(claims["aud"], "https://web.push.apple.com", "origin only");
        assert_eq!(claims["sub"], SUBJECT);
        let exp = claims["exp"].as_u64().unwrap();
        assert!(
            exp > now && exp - now <= 24 * 3600,
            "exp within 24 h (RFC 8292 §2)"
        );
    }

    #[test]
    fn the_vapid_key_survives_a_save_and_load() {
        let vapid = Vapid::generate().unwrap();
        let again = Vapid::from_pkcs8(vapid.pkcs8()).unwrap();
        assert_eq!(vapid.public_key(), again.public_key());
        assert!(Vapid::from_pkcs8(b"not a key").is_err());
    }

    #[test]
    fn origin_keeps_scheme_host_and_port_only() {
        assert_eq!(
            origin("https://fcm.googleapis.com/fcm/send/x"),
            Some("https://fcm.googleapis.com")
        );
        assert_eq!(
            origin("https://h.example:8443?q=1"),
            Some("https://h.example:8443")
        );
        assert_eq!(origin("http://h.example/x"), None);
    }
}
