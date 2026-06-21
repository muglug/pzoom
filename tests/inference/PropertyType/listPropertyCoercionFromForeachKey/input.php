<?php

class Test {
    /** @var list<int> */
    public array $list = [];

    /** @param list{int, int} $arr */
    public function test(array $arr): void
    {
        foreach ($arr as $i => $v) {
            $this->list[$i] = $v;
        }
    }
}
