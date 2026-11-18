--TEST--
phptoken-factory-errors
--FILE--
<?php
class BadToken extends PhpToken { public $value = UNDEFINED_CONSTANT; }
abstract class AbstractToken extends PhpToken {}
foreach ([BadToken::class, AbstractToken::class] as $class) {
    try { $class::tokenize("<?php foo"); } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
try { AbstractToken::tokenize(""); } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Undefined constant "UNDEFINED_CONSTANT"
Cannot instantiate abstract class AbstractToken
Cannot instantiate abstract class AbstractToken
