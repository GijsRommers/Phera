--TEST--
Distinct binary and UTF-8 identifiers have independent bindings
--FILE--
<?php
$nameÿ = 'raw ff';
$nameþ = 'raw fe';
$nameÃ¿ = 'UTF-8';
function tagÿ() { return 'function'; }
echo "$nameÿ/$nameþ/$nameÃ¿/", tagÿ(), "
";
--EXPECT--
raw ff/raw fe/UTF-8/function
