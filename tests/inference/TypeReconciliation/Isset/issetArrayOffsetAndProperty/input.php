<?php
class A {
    /** @var ?B */
    public $b;
}
class B {}

/**
 * @param A[] $arr
 */
function takesAList(array $arr) : B {
    if (isset($arr[1]->b)) {
        return $arr[1]->b;
    }
    throw new \Exception("bad");
}