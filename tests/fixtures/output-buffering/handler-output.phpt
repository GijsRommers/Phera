--TEST--
Output handlers report PHP 8.5 deprecations and keep their return values
--FILE--
<?php
function decorate($bytes) {
    echo 'discarded by the return value';
    return "[" . ob_get_status()['flags'] . ":$bytes]";
}
function passthrough($bytes) {
    echo '+printed';
    return false;
}
ob_start('decorate');
echo 'first';
ob_flush();
echo 'second';
ob_end_flush();
echo "\n";
ob_start('passthrough');
echo 'third';
ob_end_flush();
echo "\n";
ob_start('decorate', 1);
echo 'chunk';
printf('%s', 'native chunk');
ob_end_clean();
ob_start('decorate');
echo 'shutdown';
--EXPECTF--

Deprecated: ob_flush(): Producing output from user output handler decorate is deprecated in %s on line 12
[32881:first]
Deprecated: ob_end_flush(): Producing output from user output handler decorate is deprecated in %s on line 14
[53361:second]

Deprecated: ob_end_flush(): Producing output from user output handler passthrough is deprecated in %s on line 18
third+printed

Deprecated: main(): Producing output from user output handler decorate is deprecated in %s on line 21
[32881:chunk]
Deprecated: printf(): Producing output from user output handler decorate is deprecated in %s on line 22
[53361:native chunk]
Deprecated: ob_end_clean(): Producing output from user output handler decorate is deprecated in %s on line 23

Deprecated: PHP Request Shutdown: Producing output from user output handler decorate is deprecated in Unknown on line 0
[32881:shutdown]
