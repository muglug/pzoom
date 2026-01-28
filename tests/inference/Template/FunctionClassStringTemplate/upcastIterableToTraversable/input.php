<?php
/**
 * @template T as iterable
 * @param class-string<T> $class
 */
function foo(string $class) : void {
    $a = new $class();

    foreach ($a as $b) {}
}