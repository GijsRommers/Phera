--TEST--
Unserialize uses the request INI depth setting and the per-call override
--FILE--
<?php
$data = 'a:1:{i:0;a:1:{i:0;a:0:{}}}';
var_dump(ini_get('unserialize_max_depth'));
var_dump(ini_set('unserialize_max_depth', '1'));
var_dump(unserialize($data));
var_dump(unserialize($data, ['max_depth' => 2]));
?>
--EXPECT--
string(4) "4096"
string(4) "4096"

Warning: unserialize(): Maximum depth of 1 exceeded. The depth limit can be changed using the max_depth unserialize() option or the unserialize_max_depth ini setting in /private/tmp/phera-serialize-fixture-depth.php on line 5

Warning: unserialize(): Error at offset 14 of 26 bytes in /private/tmp/phera-serialize-fixture-depth.php on line 5
bool(false)
array(1) {
  [0]=>
  array(1) {
    [0]=>
    array(0) {
    }
  }
}
