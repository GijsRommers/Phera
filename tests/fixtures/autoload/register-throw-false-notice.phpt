--TEST--
Register throw false notice
--FILE--
<?php
function actual($class) {} spl_autoload_register("actual", false);
--EXPECTF--

Notice: spl_autoload_register(): Argument #2 ($do_throw) has been ignored, spl_autoload_register() will always throw in %s/register-throw-false-notice.php on line 2
