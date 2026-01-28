<?php
/**
 * @template T as iterable<int>
 * @param T::class $class
 */
function foo(string $class) : void {
    $a = new $class();

    foreach ($a as $b) {}
}