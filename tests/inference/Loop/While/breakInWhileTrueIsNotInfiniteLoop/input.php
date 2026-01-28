<?php
/** @return Generator<array-key, mixed> */
function f()
{
    if (rand(0,1)) {
        throw new Exception;
    }

    while (true) {
        yield 1;
        break;
    }
}
