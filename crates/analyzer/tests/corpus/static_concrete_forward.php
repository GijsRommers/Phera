<?php
// Rule: names / global scope / callable; static_concrete_forward.
class C { function run() {} function f() { C::run(); } }
