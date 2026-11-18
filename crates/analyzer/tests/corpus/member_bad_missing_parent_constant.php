<?php
// Rule: constant.notFound; member bad missing parent constant.
class P {} class C extends P { function f() { echo parent::MISSING; } } // error0
