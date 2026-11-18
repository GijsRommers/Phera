--TEST--
enums: unit enum implementer error
--FILE--
<?php
class C implements UnitEnum{static function cases():array{return [];}}
--EXPECTF--
Fatal error: Non-enum class C cannot implement interface UnitEnum in /private%s on line 2
