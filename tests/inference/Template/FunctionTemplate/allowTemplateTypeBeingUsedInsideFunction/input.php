<?php
/**
 * @template T of DateTime
 * @param callable(T) $callable
 * @param T $value
 */
function foo(callable $callable, DateTime $value) : void {
    $callable($value);
}