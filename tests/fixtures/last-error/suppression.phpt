--TEST--
Suppression preserves the latest warning and its exact source
--FILE--
<?php
@trigger_error('first', E_USER_WARNING);
@trigger_error('second', E_USER_NOTICE);
$error = error_get_last();
echo implode(',', array_keys($error)), "\n";
echo $error['type'], ":", $error['message'], ":", $error['file'], ":", $error['line'], "\n";
--EXPECTF--
type,message,file,line
1024:second:%s/suppression.php:3
