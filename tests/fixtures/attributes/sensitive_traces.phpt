--TEST--
SensitiveParameter redacts caught and uncaught string traces
--FILE--
<?php
function sensitiveFailure(#[SensitiveParameter] string $secret, string $public) {
    throw new RuntimeException('caught message');
}
try {
    sensitiveFailure('sensitive value', 'shown');
} catch (RuntimeException $exception) {
    echo $exception->getTraceAsString(), "\n";
    echo get_debug_type($exception->getTrace()[0]['args'][0]), "\n";
}
sensitiveFailure('another secret', 'visible');
--EXPECTF--
#0 %s(6): sensitiveFailure(Object(SensitiveParameterValue), 'shown')
#1 {main}
SensitiveParameterValue

Fatal error: Uncaught RuntimeException: caught message in %s:3
Stack trace:
#0 %s(11): sensitiveFailure(Object(SensitiveParameterValue), 'visible')
#1 {main}
  thrown in %s on line 3
