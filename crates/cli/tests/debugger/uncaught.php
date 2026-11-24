<?php
class UncaughtDebugFailure extends RuntimeException {}
function crash(): void {
    throw new UncaughtDebugFailure('uncaught debugger test', 23);
}
crash();
