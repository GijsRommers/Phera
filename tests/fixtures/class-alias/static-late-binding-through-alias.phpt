--TEST--
Static late binding through alias
--FILE--
<?php
class Original { static function name() { return get_called_class(); } } class_alias("Original", "Another"); var_dump(Another::name());
--EXPECT--
string(8) "Original"
