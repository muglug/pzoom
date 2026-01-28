<?php

function a(string $arg): int {
    $v = function() use (&$arg): void {
        if (is_integer($arg)) {
            echo $arg;
        }
        if (random_bytes(1)) {
            $arg = 123;
        }
    };
    $v();
    $v();
    return 0;
}

a("test");
