--TEST--
Session cookie settings and signature dispatch
--FILE--
<?php
ob_start();
var_dump(session_get_cookie_params());
var_dump(session_set_cookie_params(60, "/app", "example.test", true, false));
var_dump(session_get_cookie_params());
var_dump(session_set_cookie_params(["Lifetime" => "2", "partitioned" => true,
    "httponly" => true, "samesite" => "Lax"]));
var_dump(session_get_cookie_params());
set_error_handler(function ($type, $message) { echo $message, "\n"; return true; });
foreach ([[], [3 => true], ["other" => 1], ["lifetime" => -2]] as $options) {
    try { var_dump(session_set_cookie_params($options)); }
    catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
try { session_set_cookie_params(["path" => "/"], "/"); }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
?>
--EXPECT--
array(7) {
  ["lifetime"]=>
  int(0)
  ["path"]=>
  string(1) "/"
  ["domain"]=>
  string(0) ""
  ["secure"]=>
  bool(false)
  ["partitioned"]=>
  bool(false)
  ["httponly"]=>
  bool(false)
  ["samesite"]=>
  string(0) ""
}
bool(true)
array(7) {
  ["lifetime"]=>
  int(60)
  ["path"]=>
  string(4) "/app"
  ["domain"]=>
  string(12) "example.test"
  ["secure"]=>
  bool(true)
  ["partitioned"]=>
  bool(false)
  ["httponly"]=>
  bool(false)
  ["samesite"]=>
  string(0) ""
}
bool(true)
array(7) {
  ["lifetime"]=>
  int(2)
  ["path"]=>
  string(4) "/app"
  ["domain"]=>
  string(12) "example.test"
  ["secure"]=>
  bool(true)
  ["partitioned"]=>
  bool(true)
  ["httponly"]=>
  bool(true)
  ["samesite"]=>
  string(3) "Lax"
}
session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key
session_set_cookie_params(): Argument #1 ($lifetime_or_options) cannot contain numeric keys
session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key
session_set_cookie_params(): Argument #1 ($lifetime_or_options) contains an unrecognized key "other"
session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key
session_set_cookie_params(): CookieLifetime cannot be negative
bool(false)
session_set_cookie_params(): Argument #2 ($path) must be null when argument #1 ($lifetime_or_options) is an array
