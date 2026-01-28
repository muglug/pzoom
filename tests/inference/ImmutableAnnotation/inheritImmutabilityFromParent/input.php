<?php
/** @psalm-immutable */
abstract class SomethingImmutable {
    abstract public function someInteger() : int;
}

class MutableImplementation extends SomethingImmutable {
    private int $counter = 0;
    public function someInteger() : int {
        return ++$this->counter;
    }
}
