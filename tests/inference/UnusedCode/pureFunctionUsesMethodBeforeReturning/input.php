<?php
/** @psalm-external-mutation-free */
final class Counter {
    private int $count = 0;

    public function __construct(int $count) {
        $this->count = $count;
    }

    public function increment() : void {
        $this->count++;
    }
}

/** @psalm-pure */
function makesACounter(int $i) : Counter {
    $c = new Counter($i);
    $c->increment();
    return $c;
}
