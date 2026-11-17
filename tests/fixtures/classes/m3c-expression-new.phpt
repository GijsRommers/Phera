--TEST--
Class conformance: expression-new
--FILE--
<?php class C {} echo (new ('C'))::class;
--EXPECT--
C
