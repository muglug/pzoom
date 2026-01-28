<?php
/** @psalm-immutable */
interface SomethingImmutable {
    public function someInteger() : int;
}

class MutableImplementation implements SomethingImmutable {
    private int $counter = 0;
    public function someInteger() : int {
        return ++$this->counter;
    }
}
