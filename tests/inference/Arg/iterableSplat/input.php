<?php
/** @param iterable<int, mixed> $args */
function foo(iterable $args): int {
    return intval(...$args);
}

/** @param ArrayIterator<int, mixed> $args */
function bar(ArrayIterator $args): int {
    return intval(...$args);
}
