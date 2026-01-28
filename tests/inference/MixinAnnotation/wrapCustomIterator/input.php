<?php
/**
 * @implements Iterator<1, 2>
 */
class Subject implements Iterator {
    /**
     * the index method exists
     *
     * @param int $index
     * @return bool
     */
    public function index($index) {
        return true;
    }

    public function current() {
        return 2;
    }

    public function next() {}

    public function key() {
        return 1;
    }

    public function valid() {
        return false;
    }

    public function rewind() {}
}

$iter = new IteratorIterator(new Subject());
$b = $iter->index(0);
