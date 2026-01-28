<?php

interface i {
}
class b {
    public int $a = 123;
}


/**
 * @psalm-suppress InvalidReturnType
 * @return properties-of<$a>
 */
function test1($a) {}
/**
 * @psalm-suppress InvalidReturnType
 * @return properties-of<i&b>
 */
function test2() {}
/**
 * @psalm-suppress InvalidReturnType
 * @return properties-of<b&i>
 */
function test3() {}

/** @var i $i */
assert($i instanceof b);
$result1 = test1($i);
$result2 = test2();
$result3 = test3();
