--TEST--
enums: pure case name
--FILE--
<?php
enum E {case Alpha;} echo E::Alpha->name;
--EXPECT--
Alpha
