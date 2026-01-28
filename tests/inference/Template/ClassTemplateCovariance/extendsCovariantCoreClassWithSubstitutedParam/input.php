<?php
/**
 * @template TValue
 * @template-implements IteratorAggregate<int,TValue>
 */
class MyArray implements IteratorAggregate {
    /** @var array<int,TValue> */
    private $values = [];

    public function __construct() {
        $this->values = [];
    }

    public function getIterator() : Traversable {
        return new ArrayObject($this->values);
    }
}

class A {}
class AChild extends A {}

/** @param IteratorAggregate<int, A> $i */
function takesIteratorAggregate(IteratorAggregate $i) : void {}

/** @param MyArray<AChild> $a */
function takesMyArrayOfException(MyArray $a) : void {
    takesIteratorAggregate($a);
}