<?php
class A {
/**
 * @psalm-return (PHP_VERSION_ID is int<70300, max> ? string : int)
 */
function getSomething()
{
    return mt_rand(1, 10) > 5 ? "a value" : 42;
}

/**
 * @psalm-return (PHP_VERSION_ID is int<70100, max> ? string : int)
 */
function getSomethingElse()
{
    return mt_rand(1, 10) > 5 ? "a value" : 42;
}
}

class B extends A {}

$class = new B();
$something = $class->getSomething();
$somethingElse = $class->getSomethingElse();
                
