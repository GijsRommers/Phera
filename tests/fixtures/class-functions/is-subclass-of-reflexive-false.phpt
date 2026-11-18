--TEST--
Is subclass of reflexive false
--FILE--
<?php
var_dump(is_subclass_of("stdClass","STDCLASS"));
--EXPECT--
bool(false)
