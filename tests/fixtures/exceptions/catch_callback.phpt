--TEST--
Exceptions: catch_callback
--FILE--
<?php
function callback($value){throw new Exception("callback");} try {call_user_func("callback",1);} catch(Throwable $error){echo $error->getMessage();}
--EXPECTF--
callback