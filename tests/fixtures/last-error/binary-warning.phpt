--TEST--
A native warning keeps non-UTF-8 path bytes in the handler and last error
--FILE--
<?php
set_error_handler(function ($level, $message) {
    echo bin2hex($message), "\n";
    return false;
});
@fopen("phera-missing-\xff-file", 'r');
echo bin2hex(error_get_last()['message']), "\n";
--EXPECT--
666f70656e2870686572612d6d697373696e672dff2d66696c65293a204661696c656420746f206f70656e2073747265616d3a204e6f20737563682066696c65206f72206469726563746f7279
666f70656e2870686572612d6d697373696e672dff2d66696c65293a204661696c656420746f206f70656e2073747265616d3a204e6f20737563682066696c65206f72206469726563746f7279
