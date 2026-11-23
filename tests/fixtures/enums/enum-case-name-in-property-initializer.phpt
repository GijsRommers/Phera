--TEST--
enums: enum case name in property initializer
--FILE--
<?php
enum E{case A;} class C{public string $name=E::A->name;} echo (new C)->name;
--EXPECT--
A
