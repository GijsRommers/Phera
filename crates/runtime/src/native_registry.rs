//! The CLI and HTTP publish different PHP built-ins. A process can run
//! `phera --dump empty.php` before it needs to register either inventory.

use std::sync::{Arc, LazyLock};

use phera_ext_http::Sapi;
use phera_extension_api::Registry;
use phera_vm::Vm;

use crate::{adapters, http};

static CLI: LazyLock<Arc<Registry>> = LazyLock::new(|| Arc::new(Registry::deferred(register_cli)));
static WEB: LazyLock<Arc<Registry>> = LazyLock::new(|| Arc::new(Registry::deferred(register_web)));

pub(crate) fn cli() -> Arc<Registry> {
    CLI.clone()
}

pub(crate) fn web() -> Arc<Registry> {
    WEB.clone()
}

fn register_cli(registry: &mut Registry) {
    register_common(registry);
    phera_ext_process::register_cli_process_control(registry);
    registry.constant("PHP_SAPI", Sapi::Cli.name());
}

fn register_web(registry: &mut Registry) {
    register_common(registry);
    phera_ext_http::register_web(registry);
    registry.sapi_ini("phera_server");
}

fn register_common(registry: &mut Registry) {
    registry.load(&phera_ext_core::Core);
    registry.class(phera_extension_api::ClassEntry {
        parent: Some("LogicException"),
        ..phera_extension_api::ClassEntry::class("Phera\\NotSupportedException")
    });
    // Thrown by `Phera\snapshot()` when the state cannot be kept.
    registry.class(phera_extension_api::ClassEntry {
        parent: Some("RuntimeException"),
        ..phera_extension_api::ClassEntry::class("Phera\\SnapshotException")
    });
    // Thrown in strict AOT image mode when a forbidden dynamic feature runs.
    registry.class(phera_extension_api::ClassEntry {
        parent: Some("RuntimeException"),
        ..phera_extension_api::ClassEntry::class("Phera\\SealedImageException")
    });
    registry.fork_state::<phera_extension_api::ImageSeal>(
        phera_extension_api::copy_state::<phera_extension_api::ImageSeal>,
    );
    registry.load(&phera_ext_standard::Standard);
    registry.load(&phera_ext_tokenizer::Tokenizer);
    registry.load(&phera_ext_streams::Streams);
    registry.load(&phera_ext_sockets::Sockets);
    registry.load(&phera_ext_process::Process);
    registry.load(&phera_ext_process::Posix);
    registry.load(&phera_ext_zlib::Zlib);
    registry.load(&phera_ext_bcmath::BcMath);
    registry.load(&phera_ext_gmp::Gmp);
    registry.load(&phera_ext_ctype::Ctype);
    registry.load(&phera_ext_date::Date);
    registry.load(&phera_ext_curl::Curl);
    registry.load(&phera_ext_dom::Libxml);
    registry.load(&phera_ext_dom::Dom);
    registry.load(&phera_ext_xml::Xml);
    registry.load(&phera_ext_xmlreader::XmlReader);
    registry.load(&phera_ext_xmlwriter::XmlWriter);
    registry.load(&phera_ext_fileinfo::FileInfo);
    registry.load(&phera_ext_gd::Gd);
    registry.load(&phera_ext_gd::ImageMetadata);
    registry.load(&phera_ext_gd::ExifImageType);
    registry.load(&phera_ext_filter::Filter);
    registry.load(&phera_ext_intl::Intl);
    registry.load(&phera_ext_json::Json);
    registry.load(&phera_ext_mbstring::Mbstring);
    registry.php_module("iconv");
    registry.load(&phera_ext_pcre::Pcre);
    registry.load(&phera_ext_pdo::Pdo);
    registry.load(&phera_ext_mysqli::Mysqli);
    registry.load(&phera_ext_pgsql::Pgsql);
    registry.php_module("pdo_mysql");
    registry.php_module("pdo_pgsql");
    registry.php_module("pdo_sqlite");
    registry.load(&phera_ext_sqlite3::Sqlite3);
    registry.load(&phera_ext_redis::Redis);
    registry.load(&phera_ext_grpc::Grpc);
    registry.load(&phera_ext_http::Http);
    http::register_request_parse_body(registry);
    registry.load(&phera_ext_hash::Hash);
    registry.load(&phera_ext_openssl::OpenSsl);
    registry.load(&phera_ext_sodium::Sodium);
    registry.load(&phera_ext_random::Random);
    registry.load(&phera_ext_reflection::Reflection);
    registry.load(&phera_ext_serialize::Serialize);
    Vm::register_fiber_classes(registry);
    Vm::register_state_forks(registry);
    registry.load(&phera_ext_spl::Spl);
    registry.load(&phera_ext_simplexml::SimpleXml);
    registry.load(&phera_ext_session::Session);
    adapters::register(registry);
    crate::websocket::register(registry);
    crate::http_server::register(registry);
    crate::http_client::register(registry);
    crate::log::register(registry);
}
