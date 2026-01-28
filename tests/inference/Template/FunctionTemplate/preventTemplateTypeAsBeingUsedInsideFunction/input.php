<?php
/**
 * @template T of DateTime
 * @param callable(T) $callable
 */
function foo(callable $callable) : void {
    $callable(new \DateTime());
}
