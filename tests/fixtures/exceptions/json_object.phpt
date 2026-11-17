--TEST--
Exceptions: json_object
--FILE--
<?php
$value=json_decode('{"a":{"b":1}}');echo get_class($value),"|",get_class($value->a),"|",$value->a->b;
--EXPECTF--
stdClass|stdClass|1