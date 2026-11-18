<?php
// Rule: property.notFound; member bad missing this property.
class C { function f() { echo $this->missing; } } // error0
