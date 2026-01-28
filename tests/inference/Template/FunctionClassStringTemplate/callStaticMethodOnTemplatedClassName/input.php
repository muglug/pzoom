<?php
/**
 * @template T
 * @param class-string<T> $class
 */
function foo(string $class, array $args) : void {
    $class::bar($args);
}