--TEST--
Register method spelling
--FILE--
<?php
class Loader { static function Run($name) {} } spl_autoload_register("loader::run"); var_dump(spl_autoload_functions());
--EXPECT--
array(1) {
  [0]=>
  array(2) {
    [0]=>
    string(6) "Loader"
    [1]=>
    string(3) "Run"
  }
}
