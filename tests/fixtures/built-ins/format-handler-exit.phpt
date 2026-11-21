--TEST--
Formatted bytes follow a warning handler that exits
--FILE--
<?php
ob_start();
set_error_handler(function () {
    echo "handler\n";
    exit;
});
printf("[%s]\n", []);
echo "unreachable\n";
--EXPECT--
handler
[Array]
