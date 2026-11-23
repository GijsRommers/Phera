--TEST--
Session methods and INI changes respect active sessions and committed headers
--FILE--
<?php
$dir = tempnam(sys_get_temp_dir(), "phera-session-");
unlink($dir);
mkdir($dir);
ob_start();
session_save_path($dir);
ini_set("session.gc_probability", 0);
session_id("guard");
set_error_handler(function ($severity, $message) {
    echo $severity, ": ", $message, "\n";
    return true;
});
session_start();
var_dump(session_start());
var_dump(session_name("new"), session_id("next"), session_save_path("/"));
var_dump(session_cache_limiter("public"), session_cache_expire(23));
var_dump(session_set_cookie_params(["path" => "/"]), ini_set("session.name", "New"));
session_destroy();
ob_end_flush();
var_dump(session_start(), session_id("next"), ini_set("session.cache_limiter", ""));
restore_error_handler();
rmdir($dir);
?>
--EXPECTF--
8: session_start(): Ignoring session_start() because a session is already active (started from %s on line 13)
bool(true)
2: session_name(): Session name cannot be changed when a session is active (started from %s on line 13)
2: session_id(): Session ID cannot be changed when a session is active (started from %s on line 13)
2: session_save_path(): Session save path cannot be changed when a session is active (started from %s on line 13)
bool(false)
bool(false)
bool(false)
2: session_cache_limiter(): Session cache limiter cannot be changed when a session is active (started from %s on line 13)
2: session_cache_expire(): Session cache expiration cannot be changed when a session is active (started from %s on line 13)
bool(false)
int(180)
2: session_set_cookie_params(): Session cookie parameters cannot be changed when a session is active (started from %s on line 13)
2: ini_set(): Session ini settings cannot be changed when a session is active (started from %s on line 13)
bool(false)
bool(false)
2: session_start(): Session cannot be started after headers have already been sent (sent from %s on line 19)
2: session_id(): Session ID cannot be changed after headers have already been sent (sent from %s on line 19)
2: ini_set(): Session ini settings cannot be changed after headers have already been sent (sent from %s on line 19)
bool(false)
bool(false)
bool(false)
