--TEST--
Already linked namespace fallback and a fresh site after declaration
--FILE--
<?php
namespace App;
function linkedLength() { return strlen('abc'); }
function freshLength() { return strlen('abc'); }
echo linkedLength(), "\n";
if (true) { function strlen($value) { return 77; } }
echo linkedLength(), "\n", freshLength(), "\n", \strlen('abc'), "\n";
--EXPECT--
3
3
77
3
