<?php
class A {}
class B {}
/**
 * @param iterable<A> $p
 */
function takesIterableOfA(iterable $p): void {}

takesIterableOfA([new B]); // should complain
