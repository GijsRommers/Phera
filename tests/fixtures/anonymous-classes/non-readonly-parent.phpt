--TEST--
anonymous-classes: non readonly parent
--FILE--
<?php
class P {} new readonly class extends P {};
--EXPECTF--
Fatal error: Readonly class P@anonymous cannot extend non-readonly class P in /private%s on line 2
