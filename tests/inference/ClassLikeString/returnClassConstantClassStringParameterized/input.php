<?php
class A {}

/**
 * @return class-string<A> $s
 */
function foo(A $a) : string {
    return $a::class;
}
