--TEST--
A buffer handler cannot open another buffer
--FILE--
<?php
ob_start(function ($text, $mode) {
    ob_start();
});
echo "outside";
--EXPECTF--

Fatal error: ob_start(): Cannot use output buffering in output buffering display handlers in %s on line 3
