--TEST--
Superglobals across nested functions and variable variable isolation
--FILE--
<?php
$_GET = $_POST = $_COOKIE = $_SERVER = $_REQUEST = $_FILES = $_ENV = $_SESSION = ['x' => 4];
function outer() {
    function inner() {
        echo $_GET['x'], $_POST['x'], $_COOKIE['x'], $_SERVER['x'], $_REQUEST['x'], $_FILES['x'], $_ENV['x'], $_SESSION['x'], "\n";
        $_SESSION['x'] = 9;
    }
    inner();
    echo $_SESSION['x'], "\n";
    $name = '_GET';
    var_dump(isset($$name));
}
outer();
echo $_SESSION['x'], "\n";
--EXPECT--
44444444
9
bool(false)
9
