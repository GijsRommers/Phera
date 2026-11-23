--TEST--
Formatted output follows the error handler in direct and buffered output
--FILE--
<?php
$stream = fopen('php://output', 'w');
foreach (['printf', 'vprintf', 'fprintf', 'vfprintf'] as $function) {
    foreach ([false, true] as $buffered) {
        foreach ([false, true] as $throws) {
            echo "$function ", $buffered ? 'buffered ' : 'direct ', $throws ? "throw\n" : "return\n";
            if ($buffered) {
                ob_start();
            }
            set_error_handler(function ($level, $message) use ($throws) {
                echo "warning:$message\n";
                if ($throws) {
                    throw new RuntimeException('stopped');
                }
                return true;
            });
            try {
                match ($function) {
                    'printf' => printf('%s', [1]),
                    'vprintf' => vprintf('%s', [[1]]),
                    'fprintf' => fprintf($stream, '%s', [1]),
                    'vfprintf' => vfprintf($stream, '%s', [[1]]),
                };
            } catch (RuntimeException $error) {
                echo $error->getMessage();
            }
            restore_error_handler();
            if ($buffered) { var_dump(ob_get_clean()); } else { echo "\n"; }
        }
    }
}
--EXPECT--
printf direct return
warning:Array to string conversion
Array
printf direct throw
warning:Array to string conversion
Arraystopped
printf buffered return
string(40) "warning:Array to string conversion
Array"
printf buffered throw
string(47) "warning:Array to string conversion
Arraystopped"
vprintf direct return
warning:Array to string conversion
Array
vprintf direct throw
warning:Array to string conversion
Arraystopped
vprintf buffered return
string(40) "warning:Array to string conversion
Array"
vprintf buffered throw
string(47) "warning:Array to string conversion
Arraystopped"
fprintf direct return
warning:Array to string conversion
Array
fprintf direct throw
warning:Array to string conversion
Arraystopped
fprintf buffered return
string(40) "warning:Array to string conversion
Array"
fprintf buffered throw
string(47) "warning:Array to string conversion
Arraystopped"
vfprintf direct return
warning:Array to string conversion
Array
vfprintf direct throw
warning:Array to string conversion
Arraystopped
vfprintf buffered return
string(40) "warning:Array to string conversion
Array"
vfprintf buffered throw
string(47) "warning:Array to string conversion
Arraystopped"
