<?php
/**
 * @template T of DateTime
 * @template T2 of DateTime
 * @param callable(T): T $parameter
 * @param T2 $value
 * @return T
 */
function foo(callable $parameter, $value)
{
    return $parameter($value);
}
