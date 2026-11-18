<?php
// Rule: method.notFound; member bad missing parent method.
class P {} class C extends P { function f() { parent::missing(); } } // error0
