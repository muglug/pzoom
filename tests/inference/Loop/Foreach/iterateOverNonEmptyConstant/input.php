<?php
class A {
    const ARR = [0, 1, 2];

    public function test() : int
    {
        foreach (self::ARR as $val) {
            $max = $val;
        }

        return $max;
    }
}
