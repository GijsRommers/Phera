--TEST--
Unserialize distinguishes missing enums, missing constants and non-case constants
--FILE--
<?php
enum Job { case Pending; const Alias = self::Pending; }
foreach (['E:10:"Missing:No";', 'E:10:"stdClass:X";', 'E:8:"Job:Gone";',
    'E:9:"Job:Alias";', 'E:7:"Missing";'] as $data) {
    var_dump(unserialize($data));
}
try { unserialize('O:3:"Job":0:{}'); } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { serialize(new class {}); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
?>
--EXPECTF--

Warning: unserialize(): Class 'Missing' not found in %s on line 5

Warning: unserialize(): Error at offset 0 of 18 bytes in %s on line 5
bool(false)

Warning: unserialize(): Class 'stdClass' is not an enum in %s on line 5

Warning: unserialize(): Error at offset 0 of 18 bytes in %s on line 5
bool(false)

Warning: unserialize(): Undefined constant Job::Gone in %s on line 5

Warning: unserialize(): Error at offset 15 of 15 bytes in %s on line 5
bool(false)

Warning: unserialize(): Job::Alias is not an enum case in %s on line 5

Warning: unserialize(): Error at offset 16 of 16 bytes in %s on line 5
bool(false)

Warning: unserialize(): Invalid enum name 'Missing' (missing colon) in %s on line 5

Warning: unserialize(): Error at offset 0 of 14 bytes in %s on line 5
bool(false)
Cannot instantiate enum Job
Serialization of 'class@anonymous' is not allowed
