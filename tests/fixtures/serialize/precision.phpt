--TEST--
Serialize uses the request ini precision including shortest mode
--FILE--
<?php
var_dump(ini_get('serialize_precision'));
foreach ([3, 0, -1, 50] as $precision) {
    ini_set('serialize_precision', (string)$precision);
    echo serialize(0.00000555555555555555), ' ', serialize(1234.56789), "\n";
}
?>
--EXPECT--
string(2) "-1"
d:5.56E-6; d:1.23E+3;
d:6.0E-6; d:1.0E+3;
d:5.55555555555555E-6; d:1234.56789;
d:5.5555555555555498925567915746714930946836830116808E-6; d:1234.567890000000033978722058236598968505859375;
