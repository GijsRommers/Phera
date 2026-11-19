--TEST--
Eval class redeclaration
--INI--
fatal_error_backtraces=1
--FILE--
<?php
function redeclare(#[SensitiveParameter] $secret) {
    eval('class Repeated {} class Repeated {}');
}
redeclare('unprintable');
--EXPECTF--
Fatal error: Cannot redeclare class Repeated (previously declared in %seval-class-redeclaration.php(3) : eval()'d code:1) in %seval-class-redeclaration.php(3) : eval()'d code on line 1
Stack trace:
#0 %seval-class-redeclaration.php(3): eval()
#1 %seval-class-redeclaration.php(5): redeclare(Object(SensitiveParameterValue))
#2 {main}
