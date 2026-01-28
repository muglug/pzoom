<?php
class Foo {}
class NotFoo {}

/**
 * @template T as Foo
 * @param T $x
 * @return T
 */
function bar($x) {
    return $x;
}

bar(new NotFoo());
