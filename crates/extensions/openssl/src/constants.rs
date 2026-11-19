//! `OPENSSL_RAW_DATA` requests bytes instead of Base64 from openssl_encrypt.

use phera_extension_api::Registry;

pub(crate) const RAW_DATA: i64 = 1;
pub(crate) const ZERO_PADDING: i64 = 2;
pub(crate) const DONT_ZERO_PAD_KEY: i64 = 4;

pub(crate) fn register(registry: &mut Registry) {
    for (name, value) in [
        ("OPENSSL_RAW_DATA", RAW_DATA),
        ("OPENSSL_ZERO_PADDING", ZERO_PADDING),
        ("OPENSSL_DONT_ZERO_PAD_KEY", DONT_ZERO_PAD_KEY),
        ("OPENSSL_ALGO_SHA1", 1),
        ("OPENSSL_ALGO_MD5", 2),
        ("OPENSSL_ALGO_MD4", 3),
        ("OPENSSL_ALGO_SHA224", 6),
        ("OPENSSL_ALGO_SHA256", 7),
        ("OPENSSL_ALGO_SHA384", 8),
        ("OPENSSL_ALGO_SHA512", 9),
        ("OPENSSL_ALGO_RMD160", 10),
        ("OPENSSL_PKCS1_PADDING", 1),
        ("OPENSSL_NO_PADDING", 3),
        ("OPENSSL_PKCS1_OAEP_PADDING", 4),
        ("OPENSSL_PKCS1_PSS_PADDING", 6),
        ("OPENSSL_CIPHER_RC2_40", 0),
        ("OPENSSL_CIPHER_RC2_128", 1),
        ("OPENSSL_CIPHER_RC2_64", 2),
        ("OPENSSL_CIPHER_DES", 3),
        ("OPENSSL_CIPHER_3DES", 4),
        ("OPENSSL_CIPHER_AES_128_CBC", 5),
        ("OPENSSL_CIPHER_AES_192_CBC", 6),
        ("OPENSSL_CIPHER_AES_256_CBC", 7),
        ("OPENSSL_KEYTYPE_RSA", 0),
        ("OPENSSL_KEYTYPE_DSA", 1),
        ("OPENSSL_KEYTYPE_DH", 2),
        ("OPENSSL_KEYTYPE_EC", 3),
        ("OPENSSL_KEYTYPE_X25519", 4),
        ("OPENSSL_KEYTYPE_ED25519", 5),
        ("OPENSSL_KEYTYPE_X448", 6),
        ("OPENSSL_KEYTYPE_ED448", 7),
        ("OPENSSL_TLSEXT_SERVER_NAME", 1),
        ("X509_PURPOSE_SSL_CLIENT", 1),
        ("X509_PURPOSE_SSL_SERVER", 2),
        ("X509_PURPOSE_NS_SSL_SERVER", 3),
        ("X509_PURPOSE_SMIME_SIGN", 4),
        ("X509_PURPOSE_SMIME_ENCRYPT", 5),
        ("X509_PURPOSE_CRL_SIGN", 6),
        ("X509_PURPOSE_ANY", 7),
        ("X509_PURPOSE_OCSP_HELPER", 8),
        ("X509_PURPOSE_TIMESTAMP_SIGN", 9),
        ("OPENSSL_ENCODING_DER", 0),
        ("OPENSSL_ENCODING_SMIME", 1),
        ("OPENSSL_ENCODING_PEM", 2),
        ("OPENSSL_CMS_DETACHED", 64),
        ("OPENSSL_CMS_TEXT", 1),
        ("OPENSSL_CMS_NOINTERN", 16),
        ("OPENSSL_CMS_NOVERIFY", 32),
        ("OPENSSL_CMS_NOCERTS", 2),
        ("OPENSSL_CMS_NOATTR", 256),
        ("OPENSSL_CMS_BINARY", 128),
        ("OPENSSL_CMS_NOSIGS", 12),
        ("OPENSSL_CMS_OLDMIMETYPE", 1024),
    ] {
        registry.constant(name, value);
    }
    registry.constant(
        "OPENSSL_VERSION_TEXT",
        "Phera OpenSSL compatibility (RustCrypto)",
    );
    registry.constant("OPENSSL_VERSION_NUMBER", 0i64);
    registry.constant(
        "OPENSSL_DEFAULT_STREAM_CIPHERS",
        concat!(
            "ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES128-GCM-SHA256:",
            "ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES256-GCM-SHA384:",
            "DHE-RSA-AES128-GCM-SHA256:DHE-DSS-AES128-GCM-SHA256:",
            "DHE-RSA-AES256-GCM-SHA384:DHE-DSS-AES256-GCM-SHA384:kEDH+AESGCM:",
            "ECDHE-RSA-AES128-SHA256:ECDHE-ECDSA-AES128-SHA256:ECDHE-RSA-AES128-SHA:",
            "ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES256-SHA384:ECDHE-ECDSA-AES256-SHA384:",
            "ECDHE-RSA-AES256-SHA:ECDHE-ECDSA-AES256-SHA:DHE-RSA-AES128-SHA256:",
            "DHE-DSS-AES128-SHA256:DHE-RSA-AES128-SHA:DHE-DSS-AES128-SHA:",
            "DHE-RSA-AES256-SHA256:DHE-DSS-AES256-SHA256:DHE-RSA-AES256-SHA:",
            "DHE-DSS-AES256-SHA:AES128-GCM-SHA256:AES256-GCM-SHA384:AES128:AES256:",
            "HIGH:!SSLv2:!aNULL:!eNULL:!EXPORT:!DES:!MD5:!RC4:!ADH",
        ),
    );
}
