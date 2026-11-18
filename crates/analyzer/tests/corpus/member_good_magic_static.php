<?php
// Rule: members / constants / object contexts; member good magic static.
class C { static function __callStatic($n, $a) {} } C::anything();
