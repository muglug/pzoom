<?php
/**
 * @psalm-template ExpectedType of object
 * @psalm-param class-string<ExpectedType> $class
 * @psalm-assert array<class-string<ExpectedType>> $value
 *
 * @param array<string> $value
 * @param string                  $class
 */
function allIsAOf($value, $class): void {}

/**
 * @psalm-template T of object
 *
 * @param array<string> $value
 * @param class-string<T> $class
 *
 * @return array<class-string<T>>
 */
function f($value, $class) {
    allIsAOf($value, $class);

    return $value;
}