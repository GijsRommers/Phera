--TEST--
anonymous-classes: readonly parent
--FILE--
<?php
readonly class P {} new class extends P {};
--EXPECTF--
Fatal error: Non-readonly class P@anonymous cannot extend readonly class P in /private%s on line 2
