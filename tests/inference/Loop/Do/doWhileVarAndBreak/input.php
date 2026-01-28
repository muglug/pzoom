<?php
/** @return void */
function foo(string $b) {}

do {
    if (null === ($a = rand(0, 1) ? "hello" : null)) {
        break;
    }

    /** @psalm-suppress MixedArgument */
    foo($a);
}
while (rand(0,100) === 10);
