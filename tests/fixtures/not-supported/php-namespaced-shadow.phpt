--TEST--
A user function and constant in the local namespace take precedence
--FILE--
<?php
namespace App;
function strftime() { return 'local'; }
const MT_RAND_PHP = 'local';
echo strftime(), ' ', MT_RAND_PHP, "\n";
--EXPECT--
local local
