//! `openssl_sign($data, $signature, $key, OPENSSL_ALGO_SHA256)`
//! produces a PKCS#1 v1.5 signature for an RSA key.

use pkcs8::AssociatedOid;
use rsa::sha2::digest::{Digest, FixedOutputReset};
use rsa::{Pkcs1v15Sign, Pss};

use crate::digest::Algorithm;
use crate::key::OsRandom;

#[derive(Clone, Copy)]
pub(crate) enum Padding {
    Pkcs1,
    Pss,
}

pub(crate) enum Operation<'a> {
    Sign(&'a rsa::PrivateKey),
    Verify(&'a rsa::PublicKey, &'a [u8]),
}

pub(crate) enum Output {
    Signature(Vec<u8>),
    Verified(bool),
}

impl Operation<'_> {
    pub(crate) fn apply(
        self,
        algorithm: Algorithm,
        padding: Padding,
        data: &[u8],
    ) -> Option<Output> {
        use rsa::sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
        match algorithm {
            Algorithm::Md5 => self.run::<md5::Md5>(padding, data),
            Algorithm::Ripemd160 => self.run::<ripemd::Ripemd160>(padding, data),
            Algorithm::Sha1 => self.run::<sha1::Sha1>(padding, data),
            Algorithm::Sha224 => self.run::<Sha224>(padding, data),
            Algorithm::Sha256 => self.run::<Sha256>(padding, data),
            Algorithm::Sha384 => self.run::<Sha384>(padding, data),
            Algorithm::Sha512 => self.run::<Sha512>(padding, data),
            Algorithm::Sha512224 => self.run::<Sha512_224>(padding, data),
            Algorithm::Sha512256 => self.run::<Sha512_256>(padding, data),
            Algorithm::Sha3_224 => self.run::<sha3::Sha3_224>(padding, data),
            Algorithm::Sha3_256 => self.run::<sha3::Sha3_256>(padding, data),
            Algorithm::Sha3_384 => self.run::<sha3::Sha3_384>(padding, data),
            Algorithm::Sha3_512 => self.run::<sha3::Sha3_512>(padding, data),
        }
    }

    fn run<D: Digest + AssociatedOid + FixedOutputReset>(
        self,
        padding: Padding,
        data: &[u8],
    ) -> Option<Output> {
        let digest = D::digest(data);
        match (self, padding) {
            (Self::Sign(key), Padding::Pkcs1) => key
                .sign_with_rng(&mut OsRandom, Pkcs1v15Sign::new::<D>(), &digest)
                .ok()
                .map(Output::Signature),
            (Self::Sign(key), Padding::Pss) => key
                .sign_with_rng(&mut OsRandom, Pss::<D>::new(), &digest)
                .ok()
                .map(Output::Signature),
            (Self::Verify(key, signature), Padding::Pkcs1) => Some(Output::Verified(
                key.verify(Pkcs1v15Sign::new::<D>(), &digest, signature)
                    .is_ok(),
            )),
            (Self::Verify(key, signature), Padding::Pss) => {
                let mut scheme = Pss::<D>::new();
                scheme.salt_len = None;
                Some(Output::Verified(
                    key.verify(scheme, &digest, signature).is_ok(),
                ))
            }
        }
    }
}
