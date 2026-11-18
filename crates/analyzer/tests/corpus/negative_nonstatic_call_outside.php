<?php
// Rule: method.staticCall; deliberate single error: nonstatic_call_outside.
class C { function f() {} } C::f();
