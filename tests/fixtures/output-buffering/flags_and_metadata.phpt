--TEST--
Buffer status and permissions are observable
--FILE--
<?php
set_error_handler(function ($level, $message) {
    echo $level, ': ', $message, "\n";
    return true;
});
ob_start(null, 0, PHP_OUTPUT_HANDLER_CLEANABLE);
echo "first";
$status = ob_get_status();
$handlers = ob_list_handlers();
$level = ob_get_level();
$clean = ob_clean();
echo "second";
$flush = ob_flush();
$end = ob_end_flush();
echo "\n", json_encode([$level, $status, $handlers, $clean, $flush, $end]), "\n";
--EXPECT--
second8: ob_flush(): Failed to flush buffer of default output handler (0)
8: ob_end_flush(): Failed to send buffer of default output handler (0)

[1,{"name":"default output handler","type":0,"flags":16,"level":0,"chunk_size":0,"buffer_size":16384,"buffer_used":5},["default output handler"],true,false,false]
