<?php
/** @psalm-external-mutation-free */
class Counter {
    private int $count = 0;

    public function __construct(int $count) {
        $this->count = $count;
    }

    public function increment() : void {
        $this->count++;
    }

    public function incrementByTwo() : void {
        $this->count = $this->count + 2;
    }

    public function incrementByFive() : void {
        $this->count += 5;
    }
}

/** @psalm-pure */
function makesACounter(int $i) : Counter {
    $c = new Counter($i);
    $c->increment();
    $c->incrementByTwo();
    $c->incrementByFive();
    return $c;
}
