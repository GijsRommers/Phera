--TEST--
Session identifiers and request configuration
--FILE--
<?php
ob_start();
var_dump(PHP_SESSION_DISABLED, PHP_SESSION_NONE, PHP_SESSION_ACTIVE, session_status());
var_dump(session_name(), session_name("APPSESSID"), ini_get("session.name"));
var_dump(session_save_path(), ini_set("session.save_path", "/example"), session_save_path());
var_dump(session_module_name(), session_cache_limiter(""), ini_get("session.cache_limiter"));
var_dump(session_cache_expire(), session_cache_expire(30), session_cache_expire());
var_dump(session_id(), session_id("mine"), session_id());
var_dump(ini_set("session.use_cookies", false), ini_get("session.use_cookies"));
$id = session_create_id("app-,");
var_dump(strlen($id), substr($id, 0, 5), ctype_xdigit(substr($id, 5)));
foreach (["invalid/prefix", "invalid.prefix", "\x80", "a\0b", str_repeat("a", 257)] as $prefix) {
    try {
        var_dump(@session_create_id($prefix));
    } catch (ValueError $error) {
        echo $error->getMessage(), "\n";
    }
}
?>
--EXPECT--
int(0)
int(1)
int(2)
int(1)
string(9) "PHPSESSID"
string(9) "PHPSESSID"
string(9) "APPSESSID"
string(0) ""
string(0) ""
string(8) "/example"
string(5) "files"
string(7) "nocache"
string(0) ""
int(180)
int(180)
int(30)
string(0) ""
string(0) ""
string(4) "mine"
string(1) "1"
string(0) ""
int(37)
string(5) "app-,"
bool(true)
bool(false)
bool(false)
bool(false)
session_create_id(): Argument #1 ($prefix) must not contain any null bytes
session_create_id(): Argument #1 ($prefix) cannot be longer than 256 characters
