--TEST--
The request_parse_body function and exception expose PHP’s signature
--FILE--
<?php
$function = new ReflectionFunction('request_parse_body');
$option = $function->getParameters()[0];
echo $function->getExtensionName(), ': ', $function->getReturnType(), PHP_EOL;
echo $option->getName(), ': ', $option->getType(), PHP_EOL;
var_dump($option->getDefaultValue());
$class = new ReflectionClass('RequestParseBodyException');
echo $class->getExtensionName(), ': ', $class->getParentClass()->getName(), PHP_EOL;
--EXPECT--
standard: array
options: ?array
NULL
Core: Exception
