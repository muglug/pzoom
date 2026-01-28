<?php
/**
 * @implements Iterator<0, mixed>
 */
class IteratorObj implements Iterator {
    function rewind(): void {}
    /** @return mixed */
    function current() { return null; }
    function key(): int { return 0; }
    function next(): void {}
    function valid(): bool { return false; }
}

function foo(\Traversable $t): void {
}

foo(new IteratorObj);
