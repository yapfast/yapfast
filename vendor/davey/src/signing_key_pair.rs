use p256::{ecdsa::SigningKey, elliptic_curve::Generate};
use rand::{rand_core::UnwrapErr, rngs::SysRng};

/// A signing key pair. This is needed if you want to pass your own key pair or store the key pair for later.
#[derive(Clone, PartialEq, Eq)]
pub struct SigningKeyPair {
  pub private: Vec<u8>,
  pub public: Vec<u8>,
}

impl SigningKeyPair {
  /// Generate a signing key pair.
  pub fn generate() -> Self {
    let signing_key = SigningKey::generate_from_rng(&mut UnwrapErr(SysRng));

    SigningKeyPair {
      private: signing_key.to_bytes().to_vec(),
      public: signing_key
        .verifying_key()
        .to_sec1_point(false)
        .to_bytes()
        .to_vec(),
    }
  }
}
