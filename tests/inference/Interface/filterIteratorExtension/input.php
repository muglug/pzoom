<?php
/**
 * @extends Iterator<mixed, mixed>
 */
interface I2 extends Iterator {}

/**
 * @extends FilterIterator<mixed, mixed, Iterator<mixed, mixed>>
 */
class DedupeIterator extends FilterIterator {
    public function __construct(I2 $i) {
        parent::__construct($i);
    }

    public function accept() : bool {
        return true;
    }
}
