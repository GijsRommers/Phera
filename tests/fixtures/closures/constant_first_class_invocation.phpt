--TEST--
constant first class invocation
--FILE--
<?php
const C=strlen(...);var_dump((C)("hello"));
--EXPECT--
int(5)
