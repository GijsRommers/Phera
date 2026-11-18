<?php
// Rule: method.staticCall; member bad nonstatic class call.
class C { function go() {} } C::go(); // error0
