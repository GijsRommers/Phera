--TEST--
include-once5
--FILE--
<?php var_dump(include_once __DIR__.'/once5.inc'); var_dump(include_once __DIR__.'/./once5.inc'); var_dump(require_once __DIR__.'/../includes/once5.inc');
--EXPECT--
loaded
int(7)
bool(true)
bool(true)
