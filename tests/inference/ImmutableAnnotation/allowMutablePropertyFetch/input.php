<?php
class B {
    public int $j = 5;
}

/**
 * @psalm-immutable
 */
class A {
    public int $i;

    public function __construct(int $i) {
        $this->i = $i;
    }

    public function getPlusOther(B $b) : int {
        return $this->i + $b->j;
    }
}
