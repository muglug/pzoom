<?php
/**
 * @template T as iterable<int>
 * @param T::class $class
 */
function foo(string $class) : void {}

function bar(Traversable $t) : void {
    foo(get_class($t));
}
