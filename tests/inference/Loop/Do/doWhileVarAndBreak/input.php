<?php
/** @return void */
function foo(string $b) {}

do {
    if (null === ($a = rand(0, 1) ? "hello" : null)) {
        break;
    }

    foo($a);
}
while (rand(0,100) === 10);
