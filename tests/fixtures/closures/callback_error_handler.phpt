--TEST--
callback error handler
--FILE--
<?php
$messages=[];set_error_handler(function($level,$message)use(&$messages){$messages[]=$message;return true;});trigger_error("callback warning",E_USER_WARNING);restore_error_handler();var_dump($messages);
--EXPECT--
array(1) {
  [0]=>
  string(16) "callback warning"
}
