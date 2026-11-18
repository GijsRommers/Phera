<?php
// Rule: method.notFound; member bad missing this method.
class C { function f() { $this->missing(); } } // error0
