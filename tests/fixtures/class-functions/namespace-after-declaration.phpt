--TEST--
A function before the first namespace declaration is a compile error
--FILE--
<?php
function beforeNamespace() {}
namespace App;
--EXPECTF--
Fatal error: Namespace declaration statement has to be the very first statement or after any declare call in the script in %s on line 3
