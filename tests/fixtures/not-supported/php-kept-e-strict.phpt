--TEST--
E_STRICT notices on every read with quiet existence checks and listings
--FILE--
<?php
var_dump(defined('E_STRICT'), defined('\E_STRICT'));
var_dump(get_defined_constants()['E_STRICT']);
var_dump(get_defined_constants(true)['Core']['E_STRICT']);
for ($i = 0; $i < 2; ++$i) {
    var_dump(E_STRICT);
}
var_dump(constant('\E_STRICT'));
--EXPECTF--
bool(true)
bool(true)
int(2048)
int(2048)

Deprecated: Constant E_STRICT is deprecated since 8.4, the error level was removed in /private%s on line 6
int(2048)

Deprecated: Constant E_STRICT is deprecated since 8.4, the error level was removed in /private%s on line 6
int(2048)

Deprecated: Constant E_STRICT is deprecated since 8.4, the error level was removed in /private%s on line 8
int(2048)
