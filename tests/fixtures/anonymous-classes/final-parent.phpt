--TEST--
anonymous-classes: final parent
--FILE--
<?php
final class P {} new class extends P {};
--EXPECTF--
Fatal error: Class P@anonymous cannot extend final class P in %s on line 2
