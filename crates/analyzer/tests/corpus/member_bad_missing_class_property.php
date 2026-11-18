<?php
// Rule: property.notFound; member bad missing class property.
class C {} echo C::$missing; // error0
