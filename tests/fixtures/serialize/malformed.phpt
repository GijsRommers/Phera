--TEST--
Unserialize reports the bytes consumed before malformed input or trailing data
--FILE--
<?php
foreach (['', 'N;extra', 'i:+0005;', 'S:2:"\00a";', 'i:9223372036854775808;',
    's:999999999999:"a";', 'a:2000000000:{N;}', 'a:1:{i:0;}', 'a:1:{i:0;r:1;}',
    'O:8:"stdClass":', 'O:8:"stdClass":0', 'E:8:"NoColon!";'] as $data) {
    $value = unserialize($data);
    echo bin2hex(serialize($value)), "\n";
}
?>
--EXPECTF--
623a303b

Warning: unserialize(): Extra data starting at offset 2 of 7 bytes in %s on line 5
4e3b
693a353b

Deprecated: unserialize(): Unserializing the 'S' format is deprecated in %s on line 5
733a323a220061223b

Warning: unserialize(): Numerical result out of range in %s on line 5
693a393232333337323033363835343737353830373b

Warning: unserialize(): Error at offset 2 of 19 bytes in %s on line 5
623a303b

Warning: unserialize(): Error at offset 14 of 17 bytes in %s on line 5
623a303b

Warning: unserialize(): Unexpected end of serialized data in %s on line 5

Warning: unserialize(): Error at offset 9 of 10 bytes in %s on line 5
623a303b

Warning: unserialize(): Error at offset 13 of 14 bytes in %s on line 5
623a303b

Warning: Bad unserialize data in %s on line 5

Warning: unserialize(): Error at offset 13 of 15 bytes in %s on line 5
623a303b

Warning: unserialize(): Error at offset 16 of 16 bytes in %s on line 5
623a303b

Warning: unserialize(): Invalid enum name 'NoColon!' (missing colon) in %s on line 5

Warning: unserialize(): Error at offset 0 of 15 bytes in %s on line 5
623a303b
