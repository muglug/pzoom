<?php
/**
 * @template T
 * @template-implements ArrayAccess<int,T>
 */
class MyArray implements ArrayAccess, IteratorAggregate {
    /** @var array<int,T> */
    private $values = [];
    public function __construct() {
        $this->values = [];
    }

    /**
     * @param int $offset
     * @param T $value
     */
    public function offsetSet($offset, $value) {
        $this->values[$offset] = $value;
    }
    /**
     * @param int $offset
     * @return T
     */
    public function offsetGet($offset) {
        return $this->values[$offset];
    }
    /**
     * @param int $offset
     * @return bool
     */
    public function offsetExists($offset) {
        return isset($this->values[$offset]);
    }
    /**
     * @param int $offset
     */
    public function offsetUnset($offset) {
        unset($this->values[$offset]);
    }

    public function getIterator() : Traversable {
        return new ArrayObject($this->values);
    }
}

class A {}
class AChild extends A {}

/** @param IteratorAggregate<int, A> $i */
function expectsIteratorAggregateOfA(IteratorAggregate $i) : void {}

/** @param MyArray<AChild> $m */
function takesMyArrayOfAChild(MyArray $m) : void {
    expectsIteratorAggregateOfA($m);
}
