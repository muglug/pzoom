<?php
class Foo {}

class Bar {}

/** @param iterable<int, Foo> $foos */
function consume(iterable $foos): void {}

/** @param IteratorAggregate<int, Bar> $t */
function foo(IteratorAggregate $t) : void {
    consume($t);
}
