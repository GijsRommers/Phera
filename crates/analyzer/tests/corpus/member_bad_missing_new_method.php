<?php
// Rule: method.notFound; member bad missing new method.
class C {} (new C)->missing(); // error0
