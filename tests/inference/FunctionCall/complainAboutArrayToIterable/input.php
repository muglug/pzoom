<?php
class A {}
class B {}
/**
 * @param iterable<mixed,A> $p
 */
function takesIterableOfA(iterable $p): void {}

takesIterableOfA([new B]); // should complain
