<?php
/**
 * @param iterable<int, string> $s
 * @return Generator<int, string>
 */
function foo(iterable $s) : Traversable {
    yield from $s;
}
