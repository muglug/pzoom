<?php
function baz(int|string $_): int {
    return 1;
}

function bar(mixed $foo): int {
    return match (true) {
        is_string($foo), is_int($foo) => baz($foo),
        default => 0,
    };
}
