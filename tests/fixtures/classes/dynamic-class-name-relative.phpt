--TEST--
The dynamic class pseudo-constant follows self, static and parent in a bound closure
--FILE--
<?php
class Base {}
class Middle extends Base {}
class Leaf extends Middle {}
$fetch = function (string $member) {
    echo self::{$member}, ' ', static::{$member}, ' ', parent::{$member}, "\n";
};
$fetch->bindTo(new Leaf, Middle::class)('class');
$fetch->bindTo(null, Leaf::class)('class');
--EXPECT--
Middle Leaf Base
Leaf Leaf Middle
